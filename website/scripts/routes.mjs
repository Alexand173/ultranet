/**
 * The routes the quality scripts walk.
 *
 * Kept in one place so the accessibility scan and the hydration check cannot
 * silently cover different surfaces — a route added to one but not the other
 * would be a blind spot that looks like a pass.
 */
export const ROUTES = [
  "/",
  "/docs",
  "/docs/whitepaper",
  "/docs/layer-0-foundation",
  "/docs/unified-scalability",
  "/docs/trusted-execution",
  "/docs/real-time-finality",
  "/docs/autonomous-intelligence",
  "/login",
  "/operator",
  "/appchains",
  "/validator",
  "/download",
  "/legislation",
  "/transact",
  "/ultraverse",
  "/learn",
  "/send-ultra",
];
