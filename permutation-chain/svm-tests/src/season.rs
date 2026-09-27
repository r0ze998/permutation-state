//! `SeasonFx`: a season built through the program itself (CreateSeason,
//! AllocWorld, AllocNation, Register), and the accounts around it. Every
//! chain a season is built on gets the VRF stand-in (`vrf`): StartSeason
//! names the base queue, and the season seed comes from it.

use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::chain::Chain;
use crate::state::*;
use permutation_rules::state::WorldState;

pub struct Params {
    pub id: u64,
    /// 0 Blitz, 1 Season.
    pub preset: u8,
    pub nations: u8,
    pub fee: u64,
    pub tick_seconds: u32,
    pub market: bool,
    pub decimals: u8,
    /// The treasury deposit every member pays (0: none; market only).
    pub deposit: u64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            id: 7,
            preset: 0,
            nations: 2,
            fee: 10_000_000,
            tick_seconds: 30,
            market: true,
            decimals: 6,
            deposit: 0,
        }
    }
}

/// A member's keys and accounts (registered or about to be).
pub struct MemberFx {
    pub wallet: Keypair,
    pub session: Keypair,
    pub token: Address,
    pub member: Address,
    pub civ: u16,
}

/// An operator AI of the roster (V5 §18.2): its wallet, salt and tag.
pub struct AiFx {
    pub wallet: Keypair,
    pub civ: u16,
    pub salt: [u8; 32],
    pub tag: [u8; 32],
    /// Its index in `SeasonFx::members` once registered.
    pub member: Option<usize>,
}

pub struct SeasonFx {
    pub p: Params,
    pub program: Address,
    pub admin: Keypair,
    pub crank: Keypair,
    pub mint: Address,
    pub admin_token: Address,
    pub season: Address,
    pub vault: Address,
    pub roster: Address,
    pub chunks: Vec<Address>,
    pub nations: Vec<Address>,
    pub members: Vec<MemberFx>,
    pub ai: Vec<AiFx>,
    pub bounty: u64,
    pub bond: u64,
}

impl SeasonFx {
    /// The keys and addresses of a season, before anything is sent.
    pub fn bare(c: &mut Chain, p: Params) -> SeasonFx {
        install_vrf(c);
        let admin = c.funded();
        let crank = c.funded();
        let mint = c.mint(p.decimals);
        let admin_token = c.token_account(&mint, &admin.pubkey(), 0);
        let id = p.id.to_le_bytes();
        let program = c.program;
        let pda = |seeds: &[&[u8]]| crate::pda(&program, seeds);
        SeasonFx {
            season: pda(&[SEASON_SEED, &id]),
            vault: pda(&[VAULT_SEED, &id]),
            roster: pda(&[ROSTER_SEED, &id]),
            chunks: (0..WORLD_CHUNKS as u8)
                .map(|k| pda(&[WORLD_SEED, &id, &[k]]))
                .collect(),
            nations: (0..p.nations as u16)
                .map(|civ| pda(&[NATION_SEED, &id, &civ.to_le_bytes()]))
                .collect(),
            p,
            program,
            admin,
            crank,
            mint,
            admin_token,
            members: vec![],
            ai: vec![],
            bounty: 0,
            bond: 0,
        }
    }

    /// CreateSeason, then every world chunk and nation account.
    pub fn create(c: &mut Chain, p: Params) -> SeasonFx {
        let s = Self::bare(c, p);
        let admin = s.admin.insecure_clone();
        c.send(vec![s.create_ix(&admin.pubkey())], &[&admin])
            .expect("CreateSeason");
        s.alloc_all(c);
        s
    }

    /// A season with operator AI members in `civs` (one each): the admin
    /// escrows `civs.len() × bounty + bond`, the roster account is created,
    /// and the AIs' tags are committed in this order. They register with
    /// `register_ai`.
    pub fn create_ai(c: &mut Chain, p: Params, civs: &[u16], bounty: u64, bond: u64) -> SeasonFx {
        let s = Self::with_ai(c, p, civs, bounty, bond);
        let admin = s.admin.insecure_clone();
        c.send(
            vec![s.create_ai_ix(&admin.pubkey(), s.roster_commit())],
            &[&admin],
        )
        .expect("CreateSeason with AIs");
        s.alloc_all(c);
        s
    }

    /// `bare`, with the AIs' wallets, salts and tags, and the admin's USDC
    /// account holding exactly the escrow (nothing sent yet).
    pub fn with_ai(c: &mut Chain, p: Params, civs: &[u16], bounty: u64, bond: u64) -> SeasonFx {
        let mut s = Self::bare(c, p);
        s.bounty = bounty;
        s.bond = bond;
        for (i, civ) in civs.iter().enumerate() {
            let wallet = c.funded();
            let mut salt = [0x5a; 32];
            salt[..8].copy_from_slice(&(i as u64).to_le_bytes());
            let tag =
                permutation_rules::roster::roster_tag(s.p.id, &wallet.pubkey().to_bytes(), &salt);
            s.ai.push(AiFx {
                wallet,
                civ: *civ,
                salt,
                tag,
                member: None,
            });
        }
        c.set_balance(&s.admin_token, bounty * civs.len() as u64 + bond);
        s
    }

    /// The chain of the AIs' tags, in roster order.
    pub fn roster_chain(&self) -> [u8; 32] {
        let tags: Vec<[u8; 32]> = self.ai.iter().map(|a| a.tag).collect();
        permutation_rules::roster::roster_chain(&tags)
    }

    /// AllocWorld for every chunk and AllocNation for every nation (crank pays).
    pub fn alloc_all(&self, c: &mut Chain) {
        let crank = self.crank.insecure_clone();
        for k in 0..WORLD_CHUNKS as u8 {
            c.send(vec![self.alloc_world_ix(&crank.pubkey(), k)], &[&crank])
                .expect("AllocWorld");
        }
        for civ in 0..self.p.nations as u16 {
            c.send(vec![self.alloc_nation_ix(&crank.pubkey(), civ)], &[&crank])
                .expect("AllocNation");
        }
    }

    /// A new wallet with `usdc` in its own token account (not registered yet).
    pub fn new_member(&self, c: &mut Chain, civ: u16, usdc: u64) -> MemberFx {
        let wallet = c.funded();
        let session = Keypair::new();
        let token = c.token_account(&self.mint, &wallet.pubkey(), usdc);
        MemberFx {
            member: self.member_pda(&wallet.pubkey()),
            wallet,
            session,
            token,
            civ,
        }
    }

    pub fn member_pda(&self, wallet: &Address) -> Address {
        crate::pda(
            &self.program,
            &[MEMBER_SEED, &self.p.id.to_le_bytes(), wallet.as_ref()],
        )
    }

    /// Who pays for a registration: the wallet itself, or in a season with
    /// operator AI members the crank (every join there is operator-paid,
    /// as the gateway's x402 facilitator pays).
    pub fn registration_payer(&self, wallet: &Keypair) -> Keypair {
        if self.ai.is_empty() {
            wallet.insecure_clone()
        } else {
            self.crank.insecure_clone()
        }
    }

    /// Registers a new member of `civ` (standing for every office, no votes)
    /// that deposits `deposit` (the season's deposit); returns its index.
    /// The wallet and the session key sign; `registration_payer` pays.
    pub fn register(&mut self, c: &mut Chain, civ: u16, deposit: u64) -> usize {
        let m = self.new_member(c, civ, self.p.fee + deposit);
        let payer = self.registration_payer(&m.wallet);
        let ix = self.register_ix(&m, &payer.pubkey(), deposit);
        let (wallet, session) = (m.wallet.insecure_clone(), m.session.insecure_clone());
        c.send(vec![ix], &[&payer, &wallet, &session])
            .expect("Register");
        self.members.push(m);
        self.members.len() - 1
    }

    /// Registers with a given session key (signing), candidacy,
    /// first-election votes, deposit and tag.
    pub fn register_keyed(
        &mut self,
        c: &mut Chain,
        civ: u16,
        session: &Keypair,
        stand: u8,
        votes: [u32; 4],
        deposit: u64,
        tag: [u8; 32],
    ) -> usize {
        let mut m = self.new_member(c, civ, self.p.fee + deposit);
        m.session = session.insecure_clone();
        let payer = self.registration_payer(&m.wallet);
        let key = session.pubkey().to_bytes();
        let ix = self.register_ix_with(&m, &payer.pubkey(), key, stand, votes, deposit, tag);
        let wallet = m.wallet.insecure_clone();
        c.send(vec![ix], &[&payer, &wallet, session])
            .expect("Register");
        self.members.push(m);
        self.members.len() - 1
    }

    /// Registers with a session key that is only a public key (the play
    /// server's bots), candidacy, votes, deposit and tag: sent with
    /// `send_as`, so the chain must run with sigverify off.
    pub fn register_raw(
        &mut self,
        c: &mut Chain,
        civ: u16,
        session: [u8; 32],
        stand: u8,
        votes: [u32; 4],
        deposit: u64,
        tag: [u8; 32],
    ) -> usize {
        assert!(
            !c.sigverify,
            "register_raw signs as a bare key: Chain::new_opts(false), or register_keyed"
        );
        let m = self.new_member(c, civ, self.p.fee + deposit);
        let payer = self.registration_payer(&m.wallet).pubkey();
        let ix = self.register_ix_with(&m, &payer, session, stand, votes, deposit, tag);
        c.send_as(vec![ix], &payer).expect("Register");
        self.members.push(m);
        self.members.len() - 1
    }

    /// Registers operator AI `i` with its roster tag (the crank pays);
    /// returns its member index.
    pub fn register_ai(&mut self, c: &mut Chain, i: usize) -> usize {
        let wallet = self.ai[i].wallet.insecure_clone();
        let civ = self.ai[i].civ;
        let deposit = self.p.deposit;
        let token = c.token_account(&self.mint, &wallet.pubkey(), self.p.fee + deposit);
        let m = MemberFx {
            member: self.member_pda(&wallet.pubkey()),
            wallet,
            session: Keypair::new(),
            token,
            civ,
        };
        let crank = self.crank.insecure_clone();
        let ix = self.register_ix_with(
            &m,
            &crank.pubkey(),
            m.session.pubkey().to_bytes(),
            0x0f,
            [u32::MAX; 4],
            deposit,
            self.ai[i].tag,
        );
        let (wallet, session) = (m.wallet.insecure_clone(), m.session.insecure_clone());
        c.send(vec![ix], &[&crank, &wallet, &session])
            .expect("AI registers");
        self.members.push(m);
        self.ai[i].member = Some(self.members.len() - 1);
        self.members.len() - 1
    }

    // ---- reading the season

    pub fn season(&self, c: &Chain) -> Season {
        c.load(&self.season)
    }

    pub fn member(&self, c: &Chain, i: usize) -> MemberAccount {
        c.load(&self.members[i].member)
    }

    pub fn nation(&self, c: &Chain, civ: usize) -> NationAccount {
        c.load(&self.nations[civ])
    }

    pub fn roster(&self, c: &Chain) -> RosterAccount {
        c.load(&self.roster)
    }

    pub fn meta(&self, c: &Chain) -> WorldMeta {
        WorldMeta::from_chunk0(&c.data(&self.chunks[0])).unwrap()
    }

    pub fn world(&self, c: &mut Chain) -> WorldState {
        let chunks = self.chunks.clone();
        c.with_world(&chunks, |w| w.read_world().unwrap())
    }

    /// The stored world's root (sha256 of its body).
    pub fn root(&self, c: &mut Chain) -> [u8; 32] {
        let chunks = self.chunks.clone();
        c.with_world(&chunks, |w| w.root().unwrap())
    }

    pub fn rules(&self) -> permutation_rules::Ruleset {
        crate::rules(self.p.preset, self.p.market)
    }
}

/// Registers the VRF stand-in and both oracle queues on `c` once
/// (`Chain::with_vrf`, in place): StartSeason checks the base queue and the
/// VRF program, and the drivers answer the season seed with it.
pub fn install_vrf(c: &mut Chain) {
    use solana_program_runtime::solana_sbpf::program::BuiltinFunctionDefinition;
    if c.owner(&crate::vrf::queue_base()).is_some() {
        return;
    }
    c.svm.add_builtin(
        crate::vrf::vrf_program(),
        crate::vrf::mock::MockVrf::register,
    );
    c.put(
        crate::vrf::queue_base(),
        crate::vrf::vrf_program(),
        vec![0; 1024],
    );
    c.put(
        crate::vrf::queue_er(),
        crate::vrf::vrf_program(),
        vec![0; 1024],
    );
}
