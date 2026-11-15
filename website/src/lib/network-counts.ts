/**
 * Validator count fallbacks.
 *
 * `/api/stats` reports `validator_count`, but those are validator *records*: genesis writes five
 * placeholder entries that produce blocks and are never paid (`reward_eligible: false`). The real
 * figure comes from the approval journal — a record there exists only once an applicant has been
 * activated by 2-of-3 sovereign approval — and surfaces that can fetch use
 * `fetchActiveValidatorCount` instead of these constants.
 *
 * These values are the pre-fetch fallback and must be updated when a real operator joins or leaves.
 */
export const OPERATOR_VALIDATOR_COUNT = 1;

/** Bootstrap/demo records the node still counts. `OPERATOR_VALIDATOR_COUNT + this = validator_count`. */
export const BOOTSTRAP_VALIDATOR_RECORDS = 5;

/** Secondary detail line used wherever the operator count is shown. */
export const VALIDATOR_COUNT_DETAIL =
  `${OPERATOR_VALIDATOR_COUNT} active · node reports ${OPERATOR_VALIDATOR_COUNT + BOOTSTRAP_VALIDATOR_RECORDS} records (${BOOTSTRAP_VALIDATOR_RECORDS} bootstrap)`;
