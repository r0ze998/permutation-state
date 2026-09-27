// The Frontier's enum tables and program errors in Japanese and English
// (contract §9.6; web design §9). Japanese inline (L`…`), English in
// lang/en-frontier.mjs; the tables follow the language at read time
// (lazyTable), so a switch re-renders without a reload. Faction names and
// colours are v9's (i18n.mjs CIV_NAMES, CIV_COLORS). Doctrine C shows as
// "Flame" (I-34). Every program error code of abi.mjs has a text
// (web-frontier-errors.test.mjs, W3-F, checks it; web-lang checks the
// English).
import { L, lazyTable } from '../lang.mjs';
import { CIV_COLORS, CIV_NAMES } from '../i18n.mjs';
import { ERRORS } from './abi.mjs';

export { CIV_COLORS as FACTION_COLORS };
/** Faction display name by id 0–5 (6 = neutral: camps, Free Cities). */
export const factionName = f => (f === 6 ? L`中立` : CIV_NAMES[['Aster', 'Borealis', 'Cinder', 'Dunmar', 'Ember', 'Fjordal'][f]] ?? `#${f}`);

/** The eight resources (holding stores), in kernel order. */
export const RESOURCES = lazyTable({
  Food: () => L`食料`, Wood: () => L`木材`, Stone: () => L`石材`, Ore: () => L`鉱石`,
  Horses: () => L`馬`, Gold: () => L`金`, Science: () => L`学術`, Influence: () => L`影響力`,
});
export const RESOURCE_ORDER = Object.freeze(['Food', 'Wood', 'Stone', 'Ore', 'Horses', 'Gold', 'Science', 'Influence']);

/** Units, in kernel order. */
export const UNITS = lazyTable({
  Spearman: () => L`槍兵`, Archer: () => L`弓兵`, Horseman: () => L`騎兵`, Pikeman: () => L`長槍兵`,
  Crossbowman: () => L`弩兵`, Knight: () => L`騎士`, Scout: () => L`斥候`, Settler: () => L`開拓者`,
});

/** Stances (plaintext order 0–3). Disarray is a posture state (M3), named for reports. */
export const STANCES = lazyTable({
  Hold: () => L`待機`, Assault: () => L`突撃`, Flank: () => L`側撃`, Brace: () => L`迎撃`, Disarray: () => L`混乱`,
});

/** Clash fates. */
export const FATES = lazyTable({
  Stays: () => L`戦場に残った`, Withdrew: () => L`隣の味方の地へ退いた`, Bounced: () => L`押し戻された（損失なし）`,
  Retreated: () => L`撤退比で引き返した（損失なし）`, Destroyed: () => L`壊滅した`, Routed: () => L`敗走した`,
});

/** Tiers of a holding. */
export const TIERS = lazyTable({ 0: () => L`村`, 1: () => L`町`, 2: () => L`都市`, 3: () => L`城塞` });

/** Doctrine display names (I-34: doctrine C is "Flame" in M1 UI strings). */
export const DOCTRINE_C = () => L`炎`;

/** The bell pipeline states (clock.mjs PIPELINE) and what they mean to a player. */
export const PIPELINE_TEXT = lazyTable({
  open: () => L`受付中：この鐘の到着は封印され、守り手の顔ぶれは鐘の始まりで固定されています`,
  awaitingBeacon: () => L`ビーコン待ち：鐘のビーコンが記録されると封を開けられます`,
  revealing: () => L`開封中：キーパーが封を開けています`,
  awaitingSeed: () => L`シード待ち：この鐘の乱数が公開されるのを待っています`,
  resolving: () => L`決着処理中：衝突を解決しています`,
  resolved: () => L`決着：報告を見られます`,
});

/** Season status names (effective status, fcodec.effectiveStatus). */
export const SEASON_STATUS_TEXT = lazyTable({
  Announced: () => L`予告済み`, Created: () => L`作成済み`, Seeded: () => L`開始待ち`, Running: () => L`進行中`,
  Ended: () => L`終了`, Closed: () => L`閉鎖`, Aborted: () => L`中止`, Unknown: () => L`不明`,
});

// ------------------------------------------------------------------ program errors (§5.4)
const ERROR_TEXT = {
  BadData: () => L`命令のデータが正しくありません`,
  BadAccount: () => L`口座が正しくありません`,
  BadAddress: () => L`口座のアドレスが正しい形ではありません`,
  Auth: () => L`必要な署名がありません`,
  WrongStatus: () => L`シーズンがこの操作をできる状態ではありません`,
  RulesetMismatch: () => L`ルールのハッシュが一致しません`,
  WrongRound: () => L`ビーコンのラウンドが違います`,
  NoAnchor: () => L`この鐘のビーコンがまだ記録されていません`,
  Crypto: () => L`署名の検証に失敗しました`,
  Capacity: () => L`今は空いている土地がありません`,
  SiteTaken: () => L`その区画はすでに使われています`,
  WindowClosed: () => L`開封の受付は終わりました`,
  TooEarly: () => L`まだ早すぎます`,
  Reserved14: () => L`（使われていないコード）`,
  Kernel: () => L`ルールがこの操作を認めません`,
  Archived: () => L`この鐘はすでに記録庫に移されました`,
  Bucket: () => L`操作の上限に達しました。少し待ってください`,
  NotTopLevel: () => L`直接の取引でしか実行できません`,
  Overflow: () => L`数値が大きすぎます`,
  NotOwner: () => L`あなたのものではありません`,
  Insufficient: () => L`資源か残高が足りません`,
  QueueFull: () => L`建設の列がいっぱいです`,
  NoTicket: () => L`入植希望がありません`,
  NotFinal: () => L`拠点がまだ確定していません`,
  TooManyAccounts: () => L`口座が多すぎます`,
  NotResident: () => L`この州はまだ前の鐘の決着が済んでいません`,
  ProvinceFull: () => L`この州の枠がいっぱいです`,
  HostBusy: () => L`この軍勢は別の命令を待っています`,
  Cooldown: () => L`軍勢が休息中か、体力が足りません`,
  TransitState: () => L`進軍の状態が合いません`,
  ArrivalBell: () => L`到着の鐘が道のりに合いません`,
  Path: () => L`道が正しくありません`,
  CommitMismatch: () => L`封の中身が約束と一致しません`,
  QuotaRefused: () => L`この鐘の到着枠に入れませんでした`,
  SlotMoved: () => L`到着枠が動きました。もう一度試します`,
  NeedArrivalDay: () => L`到着記録の口座が必要です`,
  Shielded: () => L`その拠点は保護中です`,
  DepartureUnsettled: () => L`出発の精算がまだです`,
  NotGathered: () => L`到着がまだ集められていません`,
  OutOfOrder: () => L`鐘の順番が違います`,
  NotQuiet: () => L`この鐘は静かではありません`,
  InputsOpen: () => L`衝突の記録はまだ閉じられません`,
  NotEligible: () => L`払い戻しの対象ではないか、すでに受け取りました`,
  FoldStale: () => L`集計が古くなっています`,
  TicketState: () => L`入植希望の状態が合いません`,
  NotDormant: () => L`まだ休眠していないか、進軍中です`,
  Explored: () => L`そこはすでに探索されています`,
  SessionExpired: () => L`ゲーム内の鍵の期限が切れました`,
  WrongRegion: () => L`地域が違います`,
  Aborted: () => L`シーズンは中止されました`,
  TipTooLow: () => L`チップが最低額に足りません`,
  AlreadyDone: () => L`すでに済んでいます`,
  LatchClosed: () => L`この鐘の到着はもう締め切られました`,
  SeedNotReady: () => L`この鐘の乱数がまだ公開されていません`,
  BadPlaintext: () => L`封の中身が正しい命令ではありません`,
  Announce: () => L`シーズンの予告が正しくありません`,
  ReservedSite: () => L`その区画は予約されています（第0・第1輪）`,
  HostInTransit: () => L`この軍勢は進軍の精算が済むまで動かせません`,
  JoinGate: () => L`参加には招待が必要です`,
  CohortFull: () => L`この州の入植希望の枠がいっぱいです。次の鐘に試してください`,
  TipNotPreset: () => L`チップは3つの選択肢から選んでください`,
  NotImplemented: () => L`まだ実装されていません`,
};

/** The text of a program error (§5.4) by code, or by name; unknown codes say so with the number. */
export function errorText(codeOrName) {
  const name = typeof codeOrName === 'number' ? ERRORS.find(e => e[0] === codeOrName)?.[1] : codeOrName;
  const fn = name && ERROR_TEXT[name];
  return fn ? fn() : L`不明なエラー（${codeOrName}）`;
}

/** Every error name that has a text (the errors test compares it with abi.mjs ERRORS). */
export const ERROR_NAMES = Object.freeze(Object.keys(ERROR_TEXT));

/** Texts for this page's own refusal codes (herald, seal, pins, wasm). */
const CLIENT_TEXT = {
  network: () => L`通信できませんでした`,
  NotFound: () => L`まだ記録がありません`,
  WrongSeason: () => L`別のシーズンの記録です`,
  WrongKey: () => L`頼んだものと違う記録が届きました`,
  NotQuicknet: () => L`このシーズンのビーコンは quicknet ではありません`,
  TestBeaconOffLocalnet: () => L`テスト用のビーコンはローカルネットでしか使えません`,
  PkHashMismatch: () => L`ビーコンの公開鍵がシーズンの記録と一致しません`,
  SealAuditFailed: () => L`封の自己点検に失敗しました。何も送っていません`,
  WasmHashMismatch: () => L`ルールのプログラムが公開されたものと一致しません`,
  PinMismatch: () => L`シーズンの口座がプログラムから導いたものと一致しません`,
  RelayMessageChanged: () => L`中継が署名前の取引を書き換えました`,
};
/** A client-side refusal's text (falls back to the program table, then the code). */
export const clientText = code => (CLIENT_TEXT[code] ? CLIENT_TEXT[code]() : errorText(code));
