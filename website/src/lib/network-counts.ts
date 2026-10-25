/**
 * Operator-verified validator count.
 *
 * The node reports `validator_count = 6`, but those are validator *records*:
 * two real operators plus four bootstrap entries written during genesis
 * (`validator_set_source: "runtime-generated during bootstrap"`). The bootstrap
 * entries are demonstration records, not independent or reachable operators, so
 * the record count must never be presented as the number of validators.
 *
 * Update this constant — not the copy — when a real operator joins or leaves.
 */
export const OPERATOR_VALIDATOR_COUNT = 2;

/** Bootstrap/demo records the node still counts. `OPERATOR_VALIDATOR_COUNT + this = validator_count`. */
export const BOOTSTRAP_VALIDATOR_RECORDS = 4;

/** Secondary detail line used wherever the operator count is shown. */
export const VALIDATOR_COUNT_DETAIL =
  `${OPERATOR_VALIDATOR_COUNT} operators · node reports ${OPERATOR_VALIDATOR_COUNT + BOOTSTRAP_VALIDATOR_RECORDS} records (${BOOTSTRAP_VALIDATOR_RECORDS} bootstrap)`;
