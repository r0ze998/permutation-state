// @permutation/game-client — see README.md and the game server's /llms.txt.
// Every module re-exported here is also a subpath export in package.json
// (test/package.test.mjs keeps the two in step).
export { GameClient } from './game.mjs';
export { GameError, HttpError, DEFAULT_SERVER, DEFAULT_GATEWAY, request, requestJson, errorCode } from './http.mjs';
export { officeOf, allowedOffices, submitOffice, splitByOffice } from './offices.mjs';
export { packBatch, encodedLen, MAX_REVEALS } from './batch.mjs';
export { joinViaX402 } from './x402-client.mjs';
export { loadOrCreateKeypair, loadOrCreateKeypairSync } from './keys.mjs';
export { commit, decisionDigest, revealOrder, clip, MAX_POLICY, MAX_RATIONALE, DEFAULT_POLICY } from './decision.mjs';
export { summarize } from './summary.mjs';
export { hexDist } from './hexgrid.mjs';
export { ChainClient, NATION_TARGET } from './chain.mjs';
export {
  encodeOrder, encodeGov, IX, IX_TAG, ROLES, NOBODY, roleMask, decodeSeason, decodeMember, decodeNationHeader, decodeWorldHeader, parseRecord,
  chainError, CHAIN_ERRORS, claimAmount, claimParts, MAGIC, NATIONS, MEMBER_KINDS, SEASON_STATUS, SEEDS, MAX_NAME, MAX_NATIONS, MAX_MEMBERS,
  MAX_GOV_PER_SIGNER, CHUNK, WORLD_HEADER, INPUT_CHUNK, BATCH_BYTES, RECORD_TAGS, MEMBER_NAMES, memberName, orderCommitment,
} from './codec.mjs';
export { seasonPda, worldChunkPda, vaultPda, nationPda, memberPda, WORLD_CHUNKS, TOKEN_PROGRAM_ID } from './pda.mjs';
export { retry, poll, sleep, isTransientRpcError, isHeavyError } from './retry.mjs';
export { TOOLS, callTool } from './tools.mjs';
// The isomorphic core (also copied into the web client, scripts/sync-web-sdk.mjs).
export { sha256 } from './sha256.mjs';
export * as base58 from './base58.mjs';
export { toHex, fromHex, toBase64, fromBase64, randomBytes, u64le, toJson } from './bytes.mjs';
export { talkBytes, MAX_TALK_CHARS, TALK_PER_TICK } from './talk.mjs';
export { signTalk, verifyTalk } from './talk-node.mjs';
export {
  pubkeyBytes, pubkeyString, isOnCurve, createProgramAddress, findProgramAddress, computeBudgetHeapFrame, compileMessage, parseMessage,
  wireTransaction, parseTransaction, messageOf, COMPUTE_BUDGET_PROGRAM, PACKET_BYTES,
} from './solana-tx.mjs';
export * as player from './player.mjs';
