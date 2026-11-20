#!/bin/bash
# Mutation checks for the Records Service's safety rules.
#
# Each mutation breaks one rule in the source, runs `cargo test --lib games::`, and expects the tests to FAIL.
# A mutation the tests do not notice means a rule that nothing guards. Sources are restored after every mutation.
# Takes about 20 minutes. Run it on a clean tree: it edits the files in place (and restores them).
#
#   tools/verify/records-mutations.sh
#
# One mutation is expected to survive on its own and is explained where it appears: the registry's "submitted day
# falls through" at the SERVICE layer alone is also enforced by the STORE layer, so removing only one layer changes
# no behaviour. The next entry removes both layers and must be caught.
cd "$(dirname "$0")/../.." || exit 2
FILES="records peer_usage registry_anchor fence chain records_config"
BAK=$(mktemp -d /tmp/mutation-backup.XXXXXX)
for f in $FILES; do cp "src/games/$f.rs" "$BAK/$f.rs"; done
cp docs/runbooks/records-service.md "$BAK/runbook.md"
total=0; caught=0; survivors=""
restore() { for f in $FILES; do cp "$BAK/$f.rs" "src/games/$f.rs"; done; cp "$BAK/runbook.md" docs/runbooks/records-service.md; }
trap restore EXIT

judge() { # label, test output
  total=$((total+1))
  if echo "$2" | grep -qE "^error(\[|:)" && ! echo "$2" | grep -q "test result"; then
    echo "[$total] $1 -> DID NOT COMPILE"; survivors="$survivors\n  $1 (did not compile)"
  elif echo "$2" | grep -q "FAILED"; then
    caught=$((caught+1)); echo "[$total] $1 -> CAUGHT ($(echo "$2" | grep -c '\.\.\. FAILED') test(s) failed)"
  else
    echo "[$total] $1 -> NOT CAUGHT"; survivors="$survivors\n  $1"
  fi
}
run() { cargo test --lib games:: -- --test-threads=1 2>&1; }

# mut <file> <nth occurrence> <old> <new> <label>
mut() {
  python3 - "$1" "$2" "$3" "$4" <<'EOF'
import sys
path, n, old, new = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
text = open(path).read()
i = -1
for _ in range(n):
    i = text.find(old, i + 1)
    if i < 0:
        print("MUTATION TARGET MISSING:", old[:70]); sys.exit(3)
open(path, "w").write(text[:i] + new + text[i + len(old):])
EOF
  if [ $? -ne 0 ]; then total=$((total+1)); echo "[$total] $5 -> TARGET MISSING (not run)"; survivors="$survivors\n  $5 (target missing)"; restore; return; fi
  judge "$5" "$(run)"; restore
}
# mutate_all <file> <old> <new> <label>: every occurrence
mut_all() {
  python3 - "$1" "$2" "$3" <<'EOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path).read()
if old not in text:
    print("MUTATION TARGET MISSING:", old[:70]); sys.exit(3)
open(path, "w").write(text.replace(old, new))
EOF
  if [ $? -ne 0 ]; then total=$((total+1)); echo "[$total] $4 -> TARGET MISSING (not run)"; survivors="$survivors\n  $4 (target missing)"; restore; return; fi
  judge "$4" "$(run)"; restore
}

P=src/games/peer_usage.rs; R=src/games/records.rs; A=src/games/registry_anchor.rs; F=src/games/fence.rs; C=src/games/chain.rs

# --- the peer allowance and its persistence
mut $P 1 "        if used >= limit {" "        if false && used >= limit {" "peer: limit check removed"
mut $P 1 "ON CONFLICT(day) DO UPDATE SET used = used + 1\",
            params![day],
        )?;
        let granted" "ON CONFLICT(day) DO UPDATE SET used = 1\",
            params![day],
        )?;
        let granted" "peer: counter reset on every reservation"
mut $P 1 "    if counter != rows {" "    if false && counter != rows {" "peer: counter/row integrity check removed"
mut $P 1 "\"UPDATE peer_anchor_roots SET state = 'submitting', tx_hash = NULL, attempts = ?4, updated_at = ?5" "\"UPDATE peer_anchor_roots SET state = state, attempts = attempts, updated_at = ?5 + 0*?4" "peer: attempt not recorded before the signer"
mut $P 1 "\"UPDATE peer_anchor_roots SET state = 'submitting', tx_hash = NULL, attempts = ?4, updated_at = ?5" "\"UPDATE peer_anchor_roots SET state = 'submitting', attempts = ?4, updated_at = ?5" "peer: a new attempt keeps a stale hash"
mut $P 1 "        if current.attempts >= max_attempts {" "        if false && current.attempts >= max_attempts {" "peer: per-root attempt cap removed"
# --- failure classification: BOTH variants (occurrence 1 is the registry flow, 2 the peer flow)
mut $R 1 "let definite = matches!(error, AnchorError::Spawn(_));" "let definite = true;" "REGISTRY failure classification: every signer failure treated as definite"
mut $R 2 "let definite = matches!(error, AnchorError::Spawn(_));" "let definite = true;" "PEER failure classification: every signer failure treated as definite"
# --- reconciliation, states and gates
mut $R 1 "        if !activity.complete || in_window {" "        if false && (!activity.complete || in_window) {" "reconciliation ignores the owner history (window expiry as proof)"
mut $R 1 "        if now < window_end + MARGIN {
            return Err(in_doubt());
        }" "" "reconciliation does not wait for the attempt's window to close"
mut $P 1 "            PeerState::Submitting => {
                return Ok(SubmitDecision::InDoubt {
                    begun_at: current.updated_at,
                });
            }" "            PeerState::Submitting => {}" "peer: a submitting root is submitted again"
mut $P 1 "            PeerState::Submitted => {
                return Ok(SubmitDecision::Wait {
                    tx_hash: current.tx_hash.unwrap_or_default(),
                });
            }" "            PeerState::Submitted => {}" "peer: a submitted root is submitted again"
mut $R 1 "        self.require_ledger_known(day, now).await?;" "" "ledger-knowledge gate removed"
mut $R 1 "        if let Some(note) = self.store.peer_hold().map_err(unusable)? {
            return Err(GamesError::ReconciliationRequired(format!(
                \"anchoring is held after a restore" "        if let Some(note) = None::<String> {
            return Err(GamesError::ReconciliationRequired(format!(
                \"anchoring is held after a restore" "restore hold ignored"
mut $R 1 "filter(|r| r.day != day) {" "filter(|r| r.day == day && false) {" "an earlier day's record of the same root ignored"
mut $C 1 "        complete: room_for_pending && reaches_back," "        complete: true," "history always reported complete"
# --- the pending-transaction guard (the node does NOT protect against a duplicate)
mut $R 1 "        if activity.transactions.iter().any(|tx| tx.pending) {
            return Err(pending_msg());
        }" "" "guard: a visible pending owner transaction no longer blocks"
mut $R 1 "        if activity.complete {
            return Ok(()); // the whole pending list was shown, and it holds none
        }" "        if activity.complete || true {
            return Ok(());
        }" "guard: a hidden pending list is never looked into"
mut $R 1 "        if account_nonce > confirmed_next {" "        if false && account_nonce > confirmed_next {" "guard: hidden pending transactions (account nonce ahead) are not refused"
mut $R 1 "        if account_nonce < confirmed_next {" "        if false && account_nonce < confirmed_next {" "guard: disagreeing node answers are not refused"
mut $R 1 "        let Some(confirmed_next) = activity.confirmed_next_nonce else {" "        let Some(confirmed_next) = activity.confirmed_next_nonce.or(Some(0)) else {" "guard: nothing to measure against is treated as zero"
mut $C 1 "        confirmed_next_nonce,
    })" "        confirmed_next_nonce: None,
    })" "chain client: the confirmed nonce is never reported"
# --- the registry state machine
mut $R 1 "        self.require_no_pending_owner_tx(&owner, now).await.map_err(text)?;" "" "registry: the pending guard is skipped"
mut $R 1 "        self.require_anchoring_allowed(&owner, &self.today(), now).await.map_err(text)?;" "" "registry: hold, fence and history gates skipped"
mut $R 1 "            PeerState::Submitted => return sync(&row.tx_hash.unwrap_or_default())," "            PeerState::Submitted => {}" "registry: submitted day falls through, SERVICE layer only (equivalent mutant: the store layer still holds)"
python3 - <<'EOF'
import re
p='src/games/records.rs'; s=open(p).read()
s=s.replace("            PeerState::Submitted => return sync(&row.tx_hash.unwrap_or_default()),","            PeerState::Submitted => {}",1); open(p,'w').write(s)
p='src/games/registry_anchor.rs'; s=open(p).read()
s=s.replace('''            PeerState::Submitted => {
                return Ok(SubmitDecision::Wait {
                    tx_hash: current.tx_hash.unwrap_or_default(),
                })
            }''','''            PeerState::Submitted => {}''',1); open(p,'w').write(s)
EOF
judge "registry: a submitted day is resubmitted (BOTH layers removed)" "$(run)"; restore
# --- the fence and the owner-history check
mut $F 1 "        let Some(path) = self.fence_path() else {
            return Ok(FenceCheck::Intact);
        };
        let (id, counter) = self.fence_values()?;
        let text" "        return Ok(FenceCheck::Intact);
        #[allow(unreachable_code)]
        let Some(path) = self.fence_path() else {
            return Ok(FenceCheck::Intact);
        };
        let (id, counter) = self.fence_values()?;
        let text" "fence check always reports intact"
mut $R 1 "        if transactions > recorded {" "        if false && transactions > recorded {" "owner-history check never fires"
mut_all $R "if ack_ts >= since" "if false" "the acknowledged baseline is ignored"
mut $P 1 "        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(Reservation::Granted(granted))" "        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(Reservation::Granted(granted))" "a reservation does not move the fence"
mut $F 1 "            \"SELECT COALESCE(SUM(attempts), 0) FROM registry_anchor_attempts WHERE updated_at >= ?1\"," "            \"SELECT 0 WHERE ?1 >= 0\"," "registry attempts are not counted"
# --- an operator's claim
mut $R 1 "    if tx.sender != owner {" "    if false && tx.sender != owner {" "validation ignores the sender"
python3 - <<'EOF'
p='src/games/records.rs'; s=open(p).read()
i=s.index('        Ok(None) => Err("it is confirmed, but the anchors index does not show'); j=s.index("\n",i)
s=s[:i]+'        Ok(None) => Ok(LandedCheck::Verified("unchecked".into())),'+s[j:]; open(p,'w').write(s)
EOF
judge "validation accepts a mined transaction the index does not show" "$(run)"; restore
mut $R 1 "        return Ok(LandedCheck::PendingClaim(" "        return Ok(LandedCheck::Verified(" "validation: a pending transaction is reported as VERIFIED"
python3 - <<'EOF'
p='src/games/records.rs'; s=open(p).read()
i=s.index('            Ok(None) => Err(doubt("it is mined but the anchors index does not show'); j=s.index("\n",i)
s=s[:i]+'            Ok(None) => Ok(InDoubtOutcome::Landed(tx_hash.to_ascii_lowercase())),'+s[j:]; open(p,'w').write(s)
EOF
judge "a claimed, mined transaction the index does not show is accepted as landed" "$(run)"; restore
mut $R 1 "        if tx.pending {
            return Err(doubt(\"it is still pending\"));
        }" "" "settle: a pending claimed transaction is not held in doubt"
mut $R 1 "        if tx.sender != owner || tx.recipient != \"appchain\" {" "        if false && (tx.sender != owner || tx.recipient != \"appchain\") {" "settle: the claimed transaction's owner is not checked"
mut $R 1 "                .reconcile_in_doubt(&owner, &day, chain_id, &root, row.updated_at, row.tx_hash.as_deref(), now)" "                .reconcile_in_doubt(&owner, &day, chain_id, &root, row.updated_at, None, now)" "a peer claim is ignored (treated as no claim)"
mut $P 1 "                if taken > 0 {" "                if false && taken > 0 {" "peer: the same transaction can be claimed for two anchors"
mut $A 1 "                if taken > 0 {" "                if false && taken > 0 {" "registry: the same transaction can be claimed for two anchors"
mut $P 1 "SET state = 'reserved', tx_hash = NULL, last_error = ?4, updated_at = ?5" "SET state = 'reserved', last_error = ?4, updated_at = ?5" "peer: clearing a claim keeps the stale hash"
mut $A 1 "SET state = 'reserved', tx_hash = NULL, last_error = ?2, updated_at = ?3 WHERE day = ?1\"" "SET state = 'reserved', last_error = ?2, updated_at = ?3 WHERE day = ?1\"" "registry: clearing a claim keeps the stale hash"
mut $P 1 "let state = if matches!(resolution, PeerResolution::Landed { .. }) { \"submitted\" } else { \"submitting\" };" "let state = \"submitted\";" "peer: a pending claim is recorded as SUBMITTED"
mut $A 1 "let state = if matches!(resolution, PeerResolution::Landed { .. }) { \"submitted\" } else { \"submitting\" };" "let state = \"submitted\";" "registry: a pending claim is recorded as SUBMITTED"
# --- documentation honesty
mut_all docs/runbooks/records-service.md "Restoring the whole state directory" "Restoring the state directory" "docs: the whole-directory limit removed from the runbook"

echo "FINAL MUTATION COUNT: caught $caught of $total"
[ -n "$survivors" ] && printf "NOT CAUGHT:%b\n" "$survivors"
echo "sources restored: $(run | grep 'test result')"
echo "MUTATIONS_DONE"
