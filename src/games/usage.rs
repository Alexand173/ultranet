//! Read-only usage reporting for the Records Service (P3).
//!
//! D3 in `docs/APPCHAIN_SERVICE_ARCHITECTURE.md` asks who should pay for anchors. This module answers
//! it from the service's own database and nothing else: how many records each application had accepted
//! per UTC day, which anchor transactions were charged, and how those charges apportion across
//! applications.
//!
//! Three rules shape every figure here.
//!
//! **An allocation set is `(chain_id, attribution_day)`.** A charge is apportioned only among the
//! applications whose records that charge actually covers. Two chains anchoring on the same day are
//! therefore two sets, and one anchor can never be counted into two sets.
//!
//! **Charges are apportioned by largest remainder, inside the set.** Each application takes its integer
//! share first; the units left unassigned go to the largest fractional remainders, ties broken by
//! ascending application id. A set's shares therefore sum to that set's charge exactly: no unit is
//! dropped, none is assigned twice, and nothing is carried over to another day.
//!
//! **A status is not a charge.** `reserved` and `submitting` mean the outcome is unknown, so they are
//! reported as unsent and add nothing to the cost. Only a `submitted` anchor — and a registry day the
//! service has since seen confirmed on the ledger — is counted as charged. The two are never conflated:
//! a submitted anchor is not called confirmed without evidence.
//!
//! Nothing here writes. There is no new table, the schema is unchanged, and the operator binary opens
//! the database with [`GamesStore::open_read_only`].

use std::collections::BTreeMap;

use chrono::{Duration as ChronoDuration, NaiveDate};
use rusqlite::params;
use serde_json::{json, Value};

use super::store::{GamesStore, StoreError};
use crate::appchain::ledger::APPCHAIN_ANCHOR_FEE;

/// The state of one anchor transaction, as this database records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorStatus {
    /// A slot was taken, but nothing was sent. Charges nothing.
    Reserved,
    /// The signer ran and the outcome is unknown. Charges nothing until resolved.
    Submitting,
    /// The signer reported a transaction hash. Charged.
    Submitted,
    /// Submitted, and the service has since seen it on the ledger. Charged, with evidence.
    Confirmed,
}

impl AnchorStatus {
    fn parse(state: &str) -> Option<Self> {
        match state {
            "reserved" => Some(AnchorStatus::Reserved),
            "submitting" => Some(AnchorStatus::Submitting),
            "submitted" => Some(AnchorStatus::Submitted),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            AnchorStatus::Reserved => "reserved",
            AnchorStatus::Submitting => "submitting",
            AnchorStatus::Submitted => "submitted",
            AnchorStatus::Confirmed => "confirmed",
        }
    }

    /// Whether this status is a submission that was accepted. Reserved and in-doubt are not.
    pub fn is_submitted_charge(self) -> bool {
        matches!(self, AnchorStatus::Submitted | AnchorStatus::Confirmed)
    }

    /// Whether this node has confirmed the anchor on the chain.
    pub fn is_confirmed_charge(self) -> bool {
        matches!(self, AnchorStatus::Confirmed)
    }

    /// Whether this status counts toward `basis`.
    pub fn counts_toward(self, basis: CostBasis) -> bool {
        match basis {
            CostBasis::Submitted => self.is_submitted_charge(),
            CostBasis::Confirmed => self.is_confirmed_charge(),
        }
    }
}

/// Which anchors a report counts as charged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostBasis {
    /// Every accepted submission, registry and peer: what the owner has committed. The default.
    Submitted,
    /// Only anchors this node has confirmed. What can be evidenced against the chain.
    Confirmed,
}

impl CostBasis {
    pub fn code(self) -> &'static str {
        match self {
            CostBasis::Submitted => "submitted",
            CostBasis::Confirmed => "confirmed",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "submitted" => Some(CostBasis::Submitted),
            "confirmed" => Some(CostBasis::Confirmed),
            _ => None,
        }
    }
}

/// A number of anchors, and what they cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CostTotals {
    pub anchors: u64,
    pub base_units: u64,
}

impl CostTotals {
    pub fn plus(self, other: CostTotals) -> CostTotals {
        CostTotals {
            anchors: self.anchors + other.anchors,
            base_units: self.base_units + other.base_units,
        }
    }
}

/// The four classes every report carries, whatever basis it was asked for.
///
/// `submitted` and `peer_without_evidence` are classes, not totals. A total is produced by
/// [`CostBreakdown::total_at`], so the headline, the per-day figures, the averages and the attribution
/// check can never be built from different definitions of "cost".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CostBreakdown {
    /// Registry anchors whose submission was accepted.
    pub submitted: CostTotals,
    /// Registry anchors this node has confirmed on the chain.
    pub confirmed: CostTotals,
    /// Registry anchors submitted but not confirmed: committed, evidence not seen here yet.
    pub awaiting: CostTotals,
    /// Peer anchors. Their confirmation is held by the Games Reward Service and is not observable in
    /// this database, so they are never counted as confirmed and never wait for evidence here.
    pub peer_without_evidence: CostTotals,
}

impl CostBreakdown {
    /// The charge this basis counts.
    ///
    /// `Submitted` includes peer: those fees are committed money, and leaving them out would make the
    /// headline smaller than the sum of the parts that produced it. `Confirmed` is narrower on purpose:
    /// it counts only what this database can evidence.
    pub fn total_at(&self, basis: CostBasis) -> CostTotals {
        match basis {
            CostBasis::Submitted => self.submitted.plus(self.peer_without_evidence),
            CostBasis::Confirmed => self.confirmed,
        }
    }

    fn plus(self, other: CostBreakdown) -> CostBreakdown {
        CostBreakdown {
            submitted: self.submitted.plus(other.submitted),
            confirmed: self.confirmed.plus(other.confirmed),
            awaiting: self.awaiting.plus(other.awaiting),
            peer_without_evidence: self.peer_without_evidence.plus(other.peer_without_evidence),
        }
    }

    fn add_registry(&mut self, status: AnchorStatus) {
        self.submitted.anchors += 1;
        self.submitted.base_units += APPCHAIN_ANCHOR_FEE;
        if status.is_confirmed_charge() {
            self.confirmed.anchors += 1;
            self.confirmed.base_units += APPCHAIN_ANCHOR_FEE;
        } else {
            self.awaiting.anchors += 1;
            self.awaiting.base_units += APPCHAIN_ANCHOR_FEE;
        }
    }

    fn add_peer(&mut self) {
        self.peer_without_evidence.anchors += 1;
        self.peer_without_evidence.base_units += APPCHAIN_ANCHOR_FEE;
    }
}

/// One anchor transaction, as far as this service can know it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorView {
    pub chain_id: u32,
    pub status: AnchorStatus,
    pub tx_hash: Option<String>,
}

/// One UTC day in the reported window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayUsage {
    pub day: String,
    pub apps: usize,
    pub records: u64,
    /// `open` before the day is closed, else the registry's own day state (`closed`, `anchored`,
    /// `confirmed`).
    pub day_state: String,
    /// A past day with records that was never closed: an operational gap, not a status.
    pub pending_close: bool,
    pub registry_anchor: Option<AnchorView>,
    /// Anchors the Games Reward Service asked for on this day, one per chain.
    pub peer_anchors: Vec<AnchorView>,
    /// The day's charges, split into the four classes. `cost.total_at(basis)` is the day's figure.
    pub cost: CostBreakdown,
    pub attributed_base_units: u64,
    pub unattributed_base_units: u64,
    pub unsent_anchors: u64,
}

/// One application's usage across the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppUsage {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub status: String,
    pub records: u64,
    pub days: usize,
    /// Records over the window's total records, in parts per million, for a display-only share.
    pub share_parts_per_million: u64,
    /// This application's apportioned share of the charges in this database, in base units.
    pub attributed_base_units: u64,
}

/// The figures D3 needs, summed over the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageTotals {
    /// The window's charges, split into the four classes. `total_at(basis)` is the headline figure.
    pub cost: CostBreakdown,
    pub registry_anchors: u64,
    pub peer_anchors: u64,
    pub records: u64,
    pub attributed_base_units: u64,
    pub unattributed_base_units: u64,
    /// Charges that covered no records at all: the cost of proving liveness.
    pub anchors_without_records: u64,
    /// Reserved or in-doubt anchors. Never counted as cost.
    pub unsent_anchors: u64,
    pub apps: usize,
    pub apps_without_records: usize,
    pub days_with_records: usize,
    pub unclosed_past_days: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageReport {
    pub from: String,
    pub to: String,
    pub today: String,
    pub registry_chain_id: Option<u32>,
    /// The basis this report was asked for. Every total, average and attribution follows it.
    pub basis: CostBasis,
    pub days: Vec<DayUsage>,
    pub apps: Vec<AppUsage>,
    /// Every charged anchor as its own `(chain_id, day)` set, with the shares that sum to its charge.
    pub sets: Vec<SetUsage>,
    pub totals: UsageTotals,
}

/// A charge to apportion, with the records it covers.
struct AllocationSet {
    chain_id: u32,
    day: String,
    status: AnchorStatus,
    fee_base_units: u64,
    records: BTreeMap<String, u64>,
}

/// One allocation set after apportionment, published so a reader can recompute it by hand.
///
/// This is the unit the report is built on: a `(chain_id, attribution_day)` pair, one set per anchor,
/// with the charge and the shares that sum to it. Two chains anchoring on one day are two sets, so a
/// charge can never be apportioned across applications it does not cover, nor counted twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetUsage {
    pub chain_id: u32,
    pub day: String,
    /// The anchor's state, so the report shows what the shares rest on.
    pub status: AnchorStatus,
    pub fee_base_units: u64,
    /// Records this charge covers, as counted in this database.
    pub records: u64,
    /// `app id -> base units`. Empty when the charge covers no records this database holds.
    pub shares: BTreeMap<String, u64>,
}

/// Build the report for `from..=to` from one consistent snapshot of the database, at the default basis
/// (charged at submission).
///
/// `today` is passed in rather than read from the clock so the report is reproducible: the same
/// database and the same window always produce the same figures.
pub fn build(
    store: &GamesStore,
    from: &str,
    to: &str,
    today: &str,
    registry_chain_id: Option<u32>,
) -> Result<UsageReport, StoreError> {
    build_at(
        store,
        from,
        to,
        today,
        registry_chain_id,
        CostBasis::Submitted,
    )
}

/// Build the report at an explicit cost basis.
///
/// The basis decides which anchors are apportioned to applications and which figure is the headline.
/// It never decides which classes are reported: all four are in every report (see [`CostBreakdown`]).
pub fn build_at(
    store: &GamesStore,
    from: &str,
    to: &str,
    today: &str,
    registry_chain_id: Option<u32>,
    basis: CostBasis,
) -> Result<UsageReport, StoreError> {
    let days = window(from, to)?;

    let connection = store.lock()?;
    // One read transaction, so the five readings below describe the same moment: a record accepted
    // between two of them could otherwise make the day's records disagree with its roots.
    connection.execute_batch("BEGIN")?;
    let read = (|| -> Result<(Records, DayStates, Vec<AnchorRow>, Vec<AnchorRow>, Vec<GameRow>), StoreError> {
        let mut records = BTreeMap::new();
        {
            let mut statement = connection.prepare(
                "SELECT day, game_id, COUNT(*) FROM registry_results
                 WHERE day >= ?1 AND day <= ?2 GROUP BY day, game_id",
            )?;
            let rows = statement.query_map(params![from, to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (day, game_id, count) = row?;
                records
                    .entry(day)
                    .or_insert_with(BTreeMap::new)
                    .insert(game_id, count.max(0) as u64);
            }
        }

        let mut day_states = BTreeMap::new();
        {
            let mut statement = connection.prepare(
                "SELECT day, state FROM registry_days WHERE day >= ?1 AND day <= ?2",
            )?;
            let rows = statement.query_map(params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (day, state) = row?;
                day_states.insert(day, state);
            }
        }

        let registry = anchors(
            &connection,
            "SELECT day, chain_id, state, tx_hash FROM registry_anchor_attempts
             WHERE day >= ?1 AND day <= ?2",
            from,
            to,
        )?;
        let peer = anchors(
            &connection,
            "SELECT day, chain_id, state, tx_hash FROM peer_anchor_roots
             WHERE day >= ?1 AND day <= ?2",
            from,
            to,
        )?;

        let mut games = Vec::new();
        {
            let mut statement =
                connection.prepare("SELECT id, name, owner, status FROM registry_games ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                Ok(GameRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    owner: row.get(2)?,
                    status: row.get(3)?,
                })
            })?;
            for row in rows {
                games.push(row?);
            }
        }
        Ok((records, day_states, registry, peer, games))
    })();
    let _ = match &read {
        Ok(_) => connection.execute_batch("COMMIT"),
        Err(_) => connection.execute_batch("ROLLBACK"),
    };
    let (records, day_states, registry_anchors, peer_anchors, games) = read?;

    // --- one row per day in the window -------------------------------------------------------------
    let mut day_rows = Vec::with_capacity(days.len());
    let mut sets: Vec<AllocationSet> = Vec::new();
    let mut unsent_anchors = 0u64;

    for day in &days {
        let day_records = records.get(day).cloned().unwrap_or_default();
        let day_state = day_states
            .get(day)
            .cloned()
            .unwrap_or_else(|| "open".to_string());
        let records_total: u64 = day_records.values().sum();

        let mut cost = CostBreakdown::default();
        let mut unsent = 0u64;

        let registry_anchor = match registry_anchors.iter().find(|row| &row.day == day) {
            Some(row) => Some(AnchorView {
                chain_id: row.chain_id,
                status: registry_status(row, &day_state)?,
                tx_hash: row.tx_hash.clone(),
            }),
            None => None,
        };

        if let Some(anchor) = &registry_anchor {
            if anchor.status.is_submitted_charge() {
                cost.add_registry(anchor.status);
                if anchor.status.counts_toward(basis) {
                    sets.push(AllocationSet {
                        chain_id: anchor.chain_id,
                        day: day.clone(),
                        status: anchor.status,
                        fee_base_units: APPCHAIN_ANCHOR_FEE,
                        records: day_records.clone(),
                    });
                }
            } else {
                unsent += 1;
            }
        }

        let mut peer = Vec::new();
        for row in peer_anchors.iter().filter(|row| &row.day == day) {
            let status =
                AnchorStatus::parse(&row.state).ok_or_else(|| unusable_status(&row.state))?;
            if status.is_submitted_charge() {
                cost.add_peer();
                // A peer anchor covers a first-party game day, whose records this service does not keep.
                // Under the confirmed basis it is counted in `peer_without_evidence` but never
                // apportioned: this database holds no evidence for it and no records to apportion it
                // against, so including it would print a share nobody can recompute.
                if status.counts_toward(basis) {
                    sets.push(AllocationSet {
                        chain_id: row.chain_id,
                        day: day.clone(),
                        status,
                        fee_base_units: APPCHAIN_ANCHOR_FEE,
                        records: BTreeMap::new(),
                    });
                }
            } else {
                unsent += 1;
            }
            peer.push(AnchorView {
                chain_id: row.chain_id,
                status,
                tx_hash: row.tx_hash.clone(),
            });
        }
        unsent_anchors += unsent;

        day_rows.push(DayUsage {
            day: day.clone(),
            apps: day_records.len(),
            records: records_total,
            pending_close: records_total > 0 && day_state == "open" && day.as_str() < today,
            day_state,
            registry_anchor,
            peer_anchors: peer,
            cost,
            attributed_base_units: 0,
            unattributed_base_units: 0,
            unsent_anchors: unsent,
        });
    }

    // --- apportion each set, and fold the result into the days and the applications ----------------
    let mut app_attributed: BTreeMap<String, u64> = BTreeMap::new();
    let mut set_rows: Vec<SetUsage> = Vec::with_capacity(sets.len());
    let mut unattributed_overall = 0u64;
    let mut anchors_without_records = 0u64;

    for set in &sets {
        let covered: u64 = set.records.values().sum();
        if set.records.is_empty() {
            unattributed_overall += set.fee_base_units;
            anchors_without_records += 1;
            if let Some(row) = day_rows.iter_mut().find(|row| row.day == set.day) {
                row.unattributed_base_units += set.fee_base_units;
            }
            set_rows.push(SetUsage {
                chain_id: set.chain_id,
                day: set.day.clone(),
                status: set.status,
                fee_base_units: set.fee_base_units,
                records: covered,
                shares: BTreeMap::new(),
            });
            continue;
        }
        let shares = allocate(set.fee_base_units, &set.records);
        let attributed: u64 = shares.values().sum();
        for (app, share) in &shares {
            *app_attributed.entry(app.clone()).or_insert(0) += *share;
        }
        unattributed_overall += set.fee_base_units - attributed;
        if let Some(row) = day_rows.iter_mut().find(|row| row.day == set.day) {
            row.attributed_base_units += attributed;
            row.unattributed_base_units += set.fee_base_units - attributed;
        }
        set_rows.push(SetUsage {
            chain_id: set.chain_id,
            day: set.day.clone(),
            status: set.status,
            fee_base_units: set.fee_base_units,
            records: covered,
            shares,
        });
    }

    // --- per application ---------------------------------------------------------------------------
    let total_records: u64 = records.values().flat_map(|apps| apps.values()).sum();
    let mut apps = Vec::with_capacity(games.len());
    for game in &games {
        let mut app_records = 0u64;
        let mut active_days = 0usize;
        for day_records in records.values() {
            if let Some(count) = day_records.get(&game.id) {
                app_records += count;
                if *count > 0 {
                    active_days += 1;
                }
            }
        }
        apps.push(AppUsage {
            id: game.id.clone(),
            name: game.name.clone(),
            owner: game.owner.clone(),
            status: game.status.clone(),
            records: app_records,
            days: active_days,
            share_parts_per_million: if total_records == 0 {
                0
            } else {
                app_records.saturating_mul(1_000_000) / total_records
            },
            attributed_base_units: app_attributed.get(&game.id).copied().unwrap_or(0),
        });
    }
    // Records from an application that is no longer registered still count, under its own id, rather
    // than disappearing from the totals.
    for (app, share) in &app_attributed {
        if !apps.iter().any(|row| &row.id == app) {
            apps.push(AppUsage {
                id: app.clone(),
                name: String::from("(unregistered)"),
                owner: String::new(),
                status: String::from("unknown"),
                records: records
                    .values()
                    .filter_map(|day_records| day_records.get(app).copied())
                    .sum(),
                days: records
                    .values()
                    .filter(|day_records| day_records.get(app).copied().unwrap_or(0) > 0)
                    .count(),
                share_parts_per_million: if total_records == 0 {
                    0
                } else {
                    records
                        .values()
                        .filter_map(|day_records| day_records.get(app).copied())
                        .sum::<u64>()
                        .saturating_mul(1_000_000)
                        / total_records
                },
                attributed_base_units: *share,
            });
        }
    }
    apps.sort_by(|left, right| {
        right
            .records
            .cmp(&left.records)
            .then_with(|| left.id.cmp(&right.id))
    });

    // Every total below is folded from the day rows, so the window cannot disagree with its own days.
    let cost: CostBreakdown = day_rows
        .iter()
        .fold(CostBreakdown::default(), |sum, row| sum.plus(row.cost));
    let attributed_total: u64 = day_rows.iter().map(|row| row.attributed_base_units).sum();
    let totals = UsageTotals {
        cost,
        registry_anchors: day_rows
            .iter()
            .filter_map(|row| row.registry_anchor.as_ref())
            .filter(|anchor| anchor.status.is_submitted_charge())
            .count() as u64,
        peer_anchors: day_rows
            .iter()
            .map(|row| {
                row.peer_anchors
                    .iter()
                    .filter(|a| a.status.is_submitted_charge())
                    .count() as u64
            })
            .sum(),
        records: total_records,
        attributed_base_units: attributed_total,
        unattributed_base_units: unattributed_overall,
        anchors_without_records,
        unsent_anchors,
        apps: apps.len(),
        apps_without_records: apps.iter().filter(|row| row.records == 0).count(),
        days_with_records: day_rows.iter().filter(|row| row.records > 0).count(),
        unclosed_past_days: day_rows.iter().filter(|row| row.pending_close).count(),
    };

    Ok(UsageReport {
        from: from.to_string(),
        to: to.to_string(),
        today: today.to_string(),
        registry_chain_id,
        basis,
        days: day_rows,
        apps,
        sets: set_rows,
        totals,
    })
}

/// A registry anchor is `confirmed` only when the service's own day state says so; otherwise the
/// attempt's own status stands. The two are never merged: a submitted anchor is not called confirmed.
fn registry_status(row: &AnchorRow, day_state: &str) -> Result<AnchorStatus, StoreError> {
    let status = AnchorStatus::parse(&row.state).ok_or_else(|| unusable_status(&row.state))?;
    Ok(
        if status == AnchorStatus::Submitted && day_state == "confirmed" {
            AnchorStatus::Confirmed
        } else {
            status
        },
    )
}

fn unusable_status(state: &str) -> StoreError {
    StoreError::Malformed(format!(
        "an anchor row carries the unknown state \"{state}\"; the report refuses to guess what it cost"
    ))
}

/// Apportion `fee` across `counts` by largest remainder.
///
/// Each application takes its integer share; the units left over go to the largest fractional
/// remainders, ties broken by ascending application id, so the result is reproducible and the shares
/// always sum to `fee` exactly.
fn allocate(fee: u64, counts: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    let total: u64 = counts.values().sum();
    let mut shares: BTreeMap<String, u64> = counts.keys().map(|id| (id.clone(), 0)).collect();
    if total == 0 || fee == 0 {
        return shares;
    }

    let mut remainders: Vec<(u128, String)> = Vec::with_capacity(counts.len());
    let mut assigned = 0u64;
    for (id, count) in counts {
        let numerator = u128::from(fee) * u128::from(*count);
        let share = (numerator / u128::from(total)) as u64;
        assigned += share;
        shares.insert(id.clone(), share);
        remainders.push((numerator % u128::from(total), id.clone()));
    }

    let mut leftover = fee - assigned;
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, id) in remainders {
        if leftover == 0 {
            break;
        }
        if let Some(share) = shares.get_mut(&id) {
            *share += 1;
            leftover -= 1;
        }
    }
    shares
}

/// Every UTC date from `from` to `to`, inclusive.
fn window(from: &str, to: &str) -> Result<Vec<String>, StoreError> {
    let start = NaiveDate::parse_from_str(from, "%Y-%m-%d").map_err(|_| bad_date(from))?;
    let end = NaiveDate::parse_from_str(to, "%Y-%m-%d").map_err(|_| bad_date(to))?;
    if start > end {
        return Err(StoreError::Malformed(format!(
            "{from} is after {to}; report a window the right way round"
        )));
    }
    let mut days = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        days.push(cursor.format("%Y-%m-%d").to_string());
        cursor += ChronoDuration::days(1);
    }
    Ok(days)
}

fn bad_date(date: &str) -> StoreError {
    StoreError::Malformed(format!("{date} is not a UTC date (expected YYYY-MM-DD)"))
}

/// What an operator asked the report to cover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportArgs {
    pub from: String,
    pub to: String,
    pub json: bool,
    pub cost_basis: CostBasis,
}

/// Parse `usage-report`'s flags.
///
/// Contradictory windows and unknown flags are refused rather than resolved. A report that silently
/// covers the wrong days, or silently prints text when JSON was asked for, looks exactly like a correct
/// one, and the operator has no way to tell afterwards.
pub fn parse_args(args: &[String], today: &str) -> Result<ReportArgs, String> {
    let mut date: Option<String> = None;
    let mut from: Option<String> = None;
    let mut to: Option<String> = None;
    let mut days: Option<String> = None;
    let mut basis: Option<String> = None;
    let mut json = false;

    // A flag may appear once. `--date A --date B` would otherwise report one of them and say nothing
    // about the other, which is the same silent-wrong-answer problem as a contradictory window.
    let mut seen: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if matches!(
            flag,
            "--json" | "--date" | "--from" | "--to" | "--days" | "--cost-basis"
        ) {
            if seen.contains(&flag) {
                return Err(format!("{flag} was given more than once; give it at most once"));
            }
            seen.push(flag);
        }
        match flag {
            "--json" => {
                json = true;
                index += 1;
            }
            "--date" | "--from" | "--to" | "--days" | "--cost-basis" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{flag} needs a value"))?
                    .clone();
                match flag {
                    "--date" => date = Some(value),
                    "--from" => from = Some(value),
                    "--to" => to = Some(value),
                    "--cost-basis" => basis = Some(value),
                    _ => days = Some(value),
                }
                index += 2;
            }
            other => {
                return Err(format!(
                    "unknown argument {other}; usage-report takes --date, --from, --to, --days, --cost-basis and --json"
                ))
            }
        }
    }

    if date.is_some() && (from.is_some() || to.is_some() || days.is_some()) {
        return Err(
            "--date selects a single day, so it cannot be combined with --from, --to or --days".into(),
        );
    }
    if days.is_some() && from.is_some() {
        return Err(
            "--days counts back from --to (or from today), so it cannot be combined with --from".into(),
        );
    }

    // The basis is validated here, before anything is opened: an unreadable value must not reach the
    // database, and a report that silently fell back to the default would answer a different question.
    let cost_basis = match basis {
        None => CostBasis::Submitted,
        Some(value) => CostBasis::parse(&value)
            .ok_or_else(|| format!("--cost-basis must be submitted or confirmed, got {value}"))?,
    };

    if let Some(date) = date {
        return Ok(ReportArgs {
            from: date.clone(),
            to: date,
            json,
            cost_basis,
        });
    }

    let end = to.unwrap_or_else(|| today.to_string());
    let start = match days {
        Some(value) => {
            let count = value
                .parse::<u64>()
                .ok()
                .filter(|count| *count > 0)
                .ok_or_else(|| format!("--days needs a positive whole number, got {value}"))?;
            days_before(&end, count.saturating_sub(1))
                .ok_or_else(|| format!("{end} is not a UTC date (expected YYYY-MM-DD)"))?
        }
        None => match from {
            Some(value) => value,
            None => days_before(&end, 6)
                .ok_or_else(|| format!("{end} is not a UTC date (expected YYYY-MM-DD)"))?,
        },
    };
    Ok(ReportArgs {
        from: start,
        to: end,
        json,
        cost_basis,
    })
}

/// The UTC date `days` days before `date`, for the report's windows.
pub fn days_before(date: &str, days: u64) -> Option<String> {
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let earlier = day.checked_sub_signed(ChronoDuration::days(i64::try_from(days).ok()?))?;
    Some(earlier.format("%Y-%m-%d").to_string())
}

/// `day -> app id -> accepted records`.
type Records = BTreeMap<String, BTreeMap<String, u64>>;
/// `day -> the registry's day state`, for the days that have been closed.
type DayStates = BTreeMap<String, String>;

struct GameRow {
    id: String,
    name: String,
    owner: String,
    status: String,
}

struct AnchorRow {
    day: String,
    chain_id: u32,
    state: String,
    tx_hash: Option<String>,
}

fn anchors(
    connection: &rusqlite::Connection,
    sql: &str,
    from: &str,
    to: &str,
) -> Result<Vec<AnchorRow>, StoreError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![from, to], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (day, chain_id, state, tx_hash) = row?;
        let chain_id = u32::try_from(chain_id).map_err(|_| {
            StoreError::Malformed(format!("a stored chain id ({chain_id}) is not a chain id"))
        })?;
        out.push(AnchorRow {
            day,
            chain_id,
            state,
            tx_hash,
        });
    }
    Ok(out)
}

// --- rendering -----------------------------------------------------------------------------------

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Base units to `$ULTRA`, six decimal places, without floating point.
fn ultra(base_units: u64) -> String {
    format!("{}.{:06}", base_units / 1_000_000, base_units % 1_000_000)
}

fn short_hash(hash: &str) -> String {
    if hash.len() <= 14 {
        return hash.to_string();
    }
    format!("{}…{}", &hash[..6], &hash[hash.len() - 6..])
}

/// What the chosen basis counts, stated once so the header and the summary cannot drift apart.
fn basis_text(basis: CostBasis) -> &'static str {
    match basis {
        CostBasis::Submitted => {
            "submitted: registry charged at submission + peer anchors without evidence"
        }
        CostBasis::Confirmed => "confirmed: registry anchors this node has confirmed only",
    }
}

/// One line of the cost summary: a class or a total, with its anchors and its base units.
fn cost_row(label: &str, totals: CostTotals) -> String {
    format!(
        "  {:<32}{:>9}{:>14}  ({} ULTRA)\n",
        label,
        grouped(totals.anchors),
        grouped(totals.base_units),
        ultra(totals.base_units)
    )
}

fn cost_totals_json(totals: CostTotals) -> Value {
    json!({ "anchors": totals.anchors, "base_units": totals.base_units })
}

/// The four classes plus the total this basis counts. The total is published rather than left to be
/// derived: a consumer that sums the wrong subset would report a cost the service never had.
fn cost_json(cost: &CostBreakdown, basis: CostBasis) -> Value {
    json!({
        "submitted": cost_totals_json(cost.submitted),
        "confirmed": cost_totals_json(cost.confirmed),
        "awaiting": cost_totals_json(cost.awaiting),
        "peer_without_evidence": cost_totals_json(cost.peer_without_evidence),
        "total_at_basis": cost_totals_json(cost.total_at(basis)),
    })
}

/// `1 day` / `2 days`, `1 anchor` / `2 anchors`. A report that says "1 anchors" invites doubt about
/// the figures beside it, so every count printed for a human uses this.
fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{} {noun}s", grouped(count))
    }
}

/// One line of the day table, header or data, from one format string: a header can never drift from
/// its column, and the single spaces between the numeric columns stop two labels touching.
fn day_columns(cells: [&str; 11]) -> String {
    format!(
        "{:<12}{:>5}{:>8}  {:<20}{:<26}{:>5} {:>9} {:>10} {:>8} {:>9} {:>10}\n",
        cells[0], cells[1], cells[2], cells[3], cells[4], cells[5], cells[6], cells[7], cells[8],
        cells[9], cells[10]
    )
}

impl UsageReport {
    /// The operator-readable report. Fixed-width blocks and figures; no interpretation.
    pub fn render_text(&self, database: &str) -> String {
        let mut out = String::new();
        let chain = self
            .registry_chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "not configured".to_string());

        out.push_str(&format!(
            "database    {database}  (read-only, one snapshot)\n"
        ));
        out.push_str(&format!(
            "range       {} .. {}   ({} day{}, {} UTC)\n",
            self.from,
            self.to,
            self.days.len(),
            if self.days.len() == 1 { "" } else { "s" },
            self.today
        ));
        out.push_str(&format!("registry    chain {chain}\n"));
        out.push_str(&format!("basis       {}\n", basis_text(self.basis)));
        // Every count on this line agrees with its noun: "1 app", "1 day", "1 anchor".
        let counted = self.totals.cost.total_at(self.basis);
        out.push_str(&format!(
            "sources     {}, {} with records, {} counted at this basis\n",
            plural(self.totals.apps as u64, "app"),
            plural(self.totals.days_with_records as u64, "day"),
            plural(counted.anchors, "anchor"),
        ));
        out.push_str("note        a per-app figure is that app's share of anchors every application shares,\n");
        out.push_str("            not a measured charge; peer anchors cover first-party game days whose records\n");
        out.push_str("            are kept by the Games Reward Service, so their cost is never apportioned to an\n");
        out.push_str("            application here\n\n");

        out.push_str(&day_columns([
            "day",
            "apps",
            "records",
            "day state",
            "registry anchor",
            "peers",
            "cost",
            "confirmed",
            "awaiting",
            "peer cost",
            "attributed",
        ]));
        for day in &self.days {
            let anchor = match &day.registry_anchor {
                Some(anchor) => {
                    let tx = anchor
                        .tx_hash
                        .as_deref()
                        .map(short_hash)
                        .unwrap_or_else(|| "-".to_string());
                    format!("chain {} {} {}", anchor.chain_id, anchor.status.code(), tx)
                }
                None => "-".to_string(),
            };
            let state = if day.pending_close {
                "open, never closed".to_string()
            } else {
                day.day_state.clone()
            };
            out.push_str(&day_columns([
                &day.day,
                &day.apps.to_string(),
                &grouped(day.records),
                &state,
                &anchor,
                &day.peer_anchors.len().to_string(),
                &grouped(day.cost.total_at(self.basis).base_units),
                &grouped(day.cost.confirmed.base_units),
                &grouped(day.cost.awaiting.base_units),
                &grouped(day.cost.peer_without_evidence.base_units),
                &grouped(day.attributed_base_units),
            ]));
        }
        out.push_str(
            "\ncost is what this basis counts: at submission, confirmed + awaiting + peer cost;\n\
             confirmed, the confirmed charge alone. Peer cost is never apportioned to an application\n\
             here: the records those anchors cover are held by the Games Reward Service.\n",
        );
        out.push_str(
            "every cost figure in that table is base units, not ULTRA: 1,000,000 base units = 1 ULTRA.\n\
             the summary below prints each total's ULTRA equivalent beside it.\n",
        );

        out.push_str("\nper app\n");
        out.push_str(&format!(
            "{:<16}{:<12}{:>10}{:>6}{:>9}{:>18}\n",
            "app", "status", "records", "days", "share", "attributed"
        ));
        for app in &self.apps {
            out.push_str(&format!(
                "{:<16}{:<12}{:>10}{:>6}{:>8.1}%{:>18}\n",
                app.id,
                app.status,
                grouped(app.records),
                app.days,
                app.share_parts_per_million as f64 / 10_000.0,
                grouped(app.attributed_base_units),
            ));
        }

        let totals = &self.totals;
        let counted = totals.cost.total_at(self.basis);
        out.push_str("\ncost summary, for D3\n");
        out.push_str(&format!("basis       {}\n", basis_text(self.basis)));
        out.push_str(&format!("{:<34}{:>9}{:>14}\n", "", "anchors", "base units"));
        // Each of the four classes appears exactly once, under the heading that says whether the
        // chosen basis counts it. That is what stops a confirmed figure being read as the whole cost.
        out.push_str("counted at this basis\n");
        match self.basis {
            CostBasis::Submitted => {
                out.push_str(&cost_row("submitted (registry)", totals.cost.submitted));
                out.push_str(&cost_row(
                    "peer_without_evidence",
                    totals.cost.peer_without_evidence,
                ));
                out.push_str(&cost_row("total at submission", counted));
            }
            CostBasis::Confirmed => {
                out.push_str(&cost_row("confirmed (registry)", totals.cost.confirmed));
                out.push_str(&cost_row("total at confirmed", counted));
            }
        }
        out.push_str("not counted at this basis\n");
        match self.basis {
            CostBasis::Submitted => {
                out.push_str(&cost_row("awaiting (registry)", totals.cost.awaiting));
            }
            CostBasis::Confirmed => {
                out.push_str(&cost_row("awaiting (registry)", totals.cost.awaiting));
                out.push_str(&cost_row(
                    "peer_without_evidence",
                    totals.cost.peer_without_evidence,
                ));
            }
        }
        out.push_str(&format!(
            "  {:<32}{:>9}{:>14}\n",
            "unsent (reserved or in doubt)",
            grouped(totals.unsent_anchors),
            "-"
        ));
        out.push_str("for reference\n");
        match self.basis {
            CostBasis::Submitted => {
                out.push_str(&cost_row("confirmed (registry)", totals.cost.confirmed));
                out.push_str(&cost_row(
                    "total at confirmed",
                    totals.cost.total_at(CostBasis::Confirmed),
                ));
            }
            CostBasis::Confirmed => {
                out.push_str(&cost_row("submitted (registry)", totals.cost.submitted));
                out.push_str(&cost_row(
                    "total at submission",
                    totals.cost.total_at(CostBasis::Submitted),
                ));
            }
        }
        out.push_str(&format!(
            "records accepted       {:>8}\n",
            grouped(totals.records)
        ));
        out.push_str(&format!(
            "cost per UTC day       {:>8} base units  (window average, at this basis)\n",
            grouped(
                counted
                    .base_units
                    .checked_div(self.days.len().max(1) as u64)
                    .unwrap_or(0)
            )
        ));
        // Derived from the attributed cost only: a peer anchor covers no records this database holds,
        // so counting its charge against these records would put the wrong number in front of D3.
        out.push_str(&format!(
            "cost per 1000 records  {:>8} base units  (attributed cost only)\n",
            grouped(
                totals
                    .attributed_base_units
                    .saturating_mul(1000)
                    .checked_div(totals.records.max(1))
                    .unwrap_or(0)
            )
        ));
        out.push_str(&format!(
            "attributed             {:>8} base units\nunattributed           {:>8} base units\n",
            grouped(totals.attributed_base_units),
            grouped(totals.unattributed_base_units)
        ));
        out.push_str(&format!(
            "anchors with no records{:>8}  (cost of proving the chain is alive)\n",
            grouped(totals.anchors_without_records)
        ));
        out.push_str(&format!(
            "apps without records   {:>8} of {}\n",
            totals.apps_without_records, totals.apps
        ));
        out.push_str(&format!(
            "past days not closed   {:>8}\n",
            grouped(totals.unclosed_past_days as u64)
        ));
        out
    }

    /// The same figures as the text, for diffing one month against the next.
    pub fn to_json(&self) -> Value {
        json!({
            "from": self.from,
            "to": self.to,
            "today": self.today,
            "registry_chain_id": self.registry_chain_id,
            "cost_basis": self.basis.code(),
            "anchor_fee_base_units": APPCHAIN_ANCHOR_FEE,
            "days": self.days.iter().map(|day| json!({
                "day": day.day,
                "apps": day.apps,
                "records": day.records,
                "day_state": day.day_state,
                "pending_close": day.pending_close,
                "registry_anchor": day.registry_anchor.as_ref().map(|anchor| json!({
                    "chain_id": anchor.chain_id,
                    "status": anchor.status.code(),
                    "tx_hash": anchor.tx_hash,
                })),
                "peer_anchors": day.peer_anchors.iter().map(|anchor| json!({
                    "chain_id": anchor.chain_id,
                    "status": anchor.status.code(),
                    "tx_hash": anchor.tx_hash,
                })).collect::<Vec<_>>(),
                "cost": cost_json(&day.cost, self.basis),
                "attributed_base_units": day.attributed_base_units,
                "unattributed_base_units": day.unattributed_base_units,
                "unsent_anchors": day.unsent_anchors,
            })).collect::<Vec<_>>(),
            "apps": self.apps.iter().map(|app| json!({
                "app": app.id,
                "name": app.name,
                "owner": app.owner,
                "status": app.status,
                "records": app.records,
                "days": app.days,
                "share_parts_per_million": app.share_parts_per_million,
                "attributed_base_units": app.attributed_base_units,
            })).collect::<Vec<_>>(),
            "allocation_sets": self.sets.iter().map(|set| json!({
                "chain_id": set.chain_id,
                "day": set.day,
                "status": set.status.code(),
                "fee_base_units": set.fee_base_units,
                "records": set.records,
                "shares": set.shares,
            })).collect::<Vec<_>>(),
            "totals": {
                "basis": self.basis.code(),
                "cost": cost_json(&self.totals.cost, self.basis),
                "registry_anchors": self.totals.registry_anchors,
                "peer_anchors": self.totals.peer_anchors,
                "records": self.totals.records,
                "attributed_base_units": self.totals.attributed_base_units,
                "unattributed_base_units": self.totals.unattributed_base_units,
                "anchors_without_records": self.totals.anchors_without_records,
                "unsent_anchors": self.totals.unsent_anchors,
                "apps": self.totals.apps,
                "apps_without_records": self.totals.apps_without_records,
                "days_with_records": self.totals.days_with_records,
                "unclosed_past_days": self.totals.unclosed_past_days,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    const A: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    const TODAY: &str = "2026-11-18";

    fn store() -> GamesStore {
        GamesStore::open(Path::new(":memory:")).unwrap()
    }

    fn insert_game(store: &GamesStore, id: &str, status: &str) {
        let connection = store.lock().unwrap();
        connection
            .execute(
                "INSERT INTO registry_games (id, name, owner, status, key_hash, created_at, updated_at)
                 VALUES (?1, ?1, ?2, ?3, x'00', 0, 0)",
                params![id, A, status],
            )
            .unwrap();
    }

    fn insert_results(store: &GamesStore, id: &str, day: &str, count: u32) {
        let connection = store.lock().unwrap();
        for index in 0..count {
            connection
                .execute(
                    "INSERT INTO registry_results
                     (game_id, player, result_id, result_id_hash, score, data_hash, day, created_at)
                     VALUES (?1, ?2, ?3, x'00', 0, x'0000000000000000000000000000000000000000000000000000000000000000', ?4, 0)",
                    params![id, A, format!("{day}-{index}"), day],
                )
                .unwrap();
        }
    }

    fn insert_day(store: &GamesStore, day: &str, state: &str, tx: Option<&str>) {
        let connection = store.lock().unwrap();
        connection
            .execute(
                "INSERT INTO registry_days
                 (day, leaves, merkle_root, day_root, state, anchor_tx, attempts, last_error, updated_at)
                 VALUES (?1, '[]', x'00', x'00', ?2, ?3, 0, NULL, 0)",
                params![day, state, tx],
            )
            .unwrap();
    }

    fn insert_registry_anchor(
        store: &GamesStore,
        day: &str,
        chain: u32,
        state: &str,
        tx: Option<&str>,
    ) {
        let root = vec![0u8; 32];
        let connection = store.lock().unwrap();
        connection
            .execute(
                "INSERT INTO registry_anchor_attempts
                 (day, chain_id, root, state, tx_hash, attempts, last_error, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, NULL, 0, 0)",
                params![day, chain, root, state, tx],
            )
            .unwrap();
    }

    fn insert_peer_anchor(
        store: &GamesStore,
        day: &str,
        chain: u32,
        seed: u8,
        state: &str,
        tx: Option<&str>,
    ) {
        let root = vec![seed; 32];
        let connection = store.lock().unwrap();
        connection
            .execute(
                "INSERT INTO peer_anchor_roots
                 (day, chain_id, root, state, tx_hash, attempts, last_error, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, NULL, 0, 0)",
                params![day, chain, root, state, tx],
            )
            .unwrap();
    }

    fn day<'a>(report: &'a UsageReport, date: &str) -> &'a DayUsage {
        report
            .days
            .iter()
            .find(|row| row.day == date)
            .unwrap_or_else(|| panic!("{date} is not in the report"))
    }

    #[test]
    fn the_window_is_inclusive_and_rejects_a_backwards_range() {
        assert_eq!(
            window("2026-11-18", "2026-11-18").unwrap(),
            vec!["2026-11-18"]
        );
        let week = window("2026-11-12", "2026-11-18").unwrap();
        assert_eq!(week.len(), 7);
        assert_eq!(week.first().unwrap(), "2026-11-12");
        assert_eq!(week.last().unwrap(), "2026-11-18");
        assert!(window("2026-11-18", "2026-11-12").is_err());
        assert!(window("yesterday", "2026-11-18").is_err());
        assert!(
            window("2026-02-30", "2026-03-01").is_err(),
            "not a real date"
        );
    }

    #[test]
    fn records_are_counted_on_the_day_they_were_accepted() {
        let store = store();
        insert_game(&store, "snake", "sandbox");
        insert_game(&store, "tetris", "listed");
        insert_results(&store, "tetris", "2026-11-16", 5);
        insert_results(&store, "snake", "2026-11-16", 3);
        insert_results(&store, "tetris", "2026-11-17", 2);

        let report = build(&store, "2026-11-16", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(day(&report, "2026-11-16").records, 8);
        assert_eq!(day(&report, "2026-11-16").apps, 2);
        assert_eq!(day(&report, "2026-11-17").records, 2);
        assert_eq!(report.totals.records, 10);

        let tetris = report.apps.iter().find(|app| app.id == "tetris").unwrap();
        assert_eq!(tetris.records, 7);
        assert_eq!(tetris.days, 2);
        assert_eq!(tetris.share_parts_per_million, 700_000);
        let snake = report.apps.iter().find(|app| app.id == "snake").unwrap();
        assert_eq!(snake.records, 3);
        assert_eq!(snake.share_parts_per_million, 300_000);
    }

    #[test]
    fn a_charge_is_apportioned_across_the_apps_whose_records_it_covers() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_game(&store, "beta", "listed");
        insert_game(&store, "gamma", "sandbox");
        insert_results(&store, "alpha", "2026-11-17", 5);
        insert_results(&store, "beta", "2026-11-17", 3);
        insert_results(&store, "gamma", "2026-11-17", 2);
        insert_day(&store, "2026-11-17", "confirmed", Some(&"ab".repeat(32)));
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", Some(&"ab".repeat(32)));

        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(report.sets.len(), 1, "one anchor, one set");
        let set = &report.sets[0];
        assert_eq!((set.chain_id, set.day.as_str()), (3, "2026-11-17"));
        assert_eq!(set.fee_base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(set.records, 10);
        assert_eq!(set.shares["alpha"], 500);
        assert_eq!(set.shares["beta"], 300);
        assert_eq!(set.shares["gamma"], 200);
        assert_eq!(set.shares.values().sum::<u64>(), APPCHAIN_ANCHOR_FEE);

        let row = day(&report, "2026-11-17");
        assert_eq!(row.cost.submitted.base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(row.cost.confirmed.base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(row.cost.awaiting.base_units, 0);
        assert_eq!(row.cost.peer_without_evidence.base_units, 0);
        assert_eq!(row.attributed_base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(row.unattributed_base_units, 0);
        assert_eq!(
            row.registry_anchor.as_ref().unwrap().status,
            AnchorStatus::Confirmed
        );
    }

    #[test]
    fn status_is_not_a_charge_and_confirmation_is_not_assumed() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-14", 1);
        insert_results(&store, "alpha", "2026-11-15", 1);
        insert_results(&store, "alpha", "2026-11-16", 1);
        insert_results(&store, "alpha", "2026-11-17", 1);

        // reserved and submitting: outcome unknown, so nothing is charged.
        insert_registry_anchor(&store, "2026-11-14", 3, "reserved", None);
        insert_registry_anchor(&store, "2026-11-15", 3, "submitting", None);
        // submitted: charged, but not called confirmed.
        insert_registry_anchor(&store, "2026-11-16", 3, "submitted", Some(&"cc".repeat(32)));
        insert_day(&store, "2026-11-16", "anchored", Some(&"cc".repeat(32)));
        // submitted and seen on the ledger: charged, with evidence.
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", Some(&"dd".repeat(32)));
        insert_day(&store, "2026-11-17", "confirmed", Some(&"dd".repeat(32)));

        let report = build(&store, "2026-11-14", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(
            day(&report, "2026-11-14")
                .cost
                .total_at(CostBasis::Submitted)
                .base_units,
            0
        );
        assert_eq!(day(&report, "2026-11-14").unsent_anchors, 1);
        assert_eq!(
            day(&report, "2026-11-15")
                .cost
                .total_at(CostBasis::Submitted)
                .base_units,
            0
        );
        assert_eq!(
            day(&report, "2026-11-16")
                .registry_anchor
                .as_ref()
                .unwrap()
                .status,
            AnchorStatus::Submitted
        );
        assert_eq!(
            day(&report, "2026-11-17")
                .registry_anchor
                .as_ref()
                .unwrap()
                .status,
            AnchorStatus::Confirmed
        );
        assert_eq!(
            report.totals.cost.total_at(CostBasis::Submitted).base_units,
            2 * APPCHAIN_ANCHOR_FEE
        );
        assert_eq!(report.totals.cost.confirmed.base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(report.totals.cost.awaiting.base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(report.totals.unsent_anchors, 2);
        assert_eq!(report.totals.cost.submitted.anchors, 2);
    }

    #[test]
    fn an_unknown_anchor_state_is_refused_rather_than_guessed() {
        // The table's own CHECK constraint is the first line of defence: an unrecognised state cannot be
        // written in the first place.
        let store = store();
        let root = vec![0u8; 32];
        let refused = store.lock().unwrap().execute(
            "INSERT INTO registry_anchor_attempts
             (day, chain_id, root, state, tx_hash, attempts, last_error, created_at, updated_at)
             VALUES ('2026-11-17', 3, ?1, 'almost-submitted', NULL, 1, NULL, 0, 0)",
            params![root],
        );
        assert!(
            refused.is_err(),
            "the schema refuses an unknown anchor state"
        );

        // And the report still refuses one it cannot interpret, rather than assuming it cost nothing.
        assert!(AnchorStatus::parse("almost-submitted").is_none());
        let error = unusable_status("almost-submitted").to_string();
        assert!(error.contains("unknown state"), "{error}");
    }

    #[test]
    fn a_peer_anchor_is_reported_on_its_request_day_and_stays_unattributed() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 4);
        insert_peer_anchor(
            &store,
            "2026-11-17",
            1,
            0x11,
            "submitted",
            Some(&"ee".repeat(32)),
        );

        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let row = day(&report, "2026-11-17");
        assert_eq!(row.peer_anchors.len(), 1);
        assert_eq!(row.peer_anchors[0].chain_id, 1);
        assert_eq!(row.peer_anchors[0].status, AnchorStatus::Submitted);
        assert_eq!(
            row.cost.total_at(CostBasis::Submitted).base_units,
            APPCHAIN_ANCHOR_FEE,
            "a peer anchor is committed money at the submitted basis"
        );
        assert_eq!(row.cost.peer_without_evidence.base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(row.cost.submitted.base_units, 0, "no registry anchor that day");
        assert_eq!(row.cost.awaiting.base_units, 0);
        assert_eq!(
            row.cost.total_at(CostBasis::Confirmed).base_units,
            0,
            "and nothing here can confirm it"
        );
        assert_eq!(row.attributed_base_units, 0, "no records for chain 1 here");
        assert_eq!(row.unattributed_base_units, APPCHAIN_ANCHOR_FEE);
        assert_eq!(report.totals.anchors_without_records, 1);
        assert_eq!(report.sets[0].shares.len(), 0);
    }

    #[test]
    fn two_chains_anchoring_on_one_day_are_two_sets() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 2);
        insert_day(&store, "2026-11-17", "confirmed", None);
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", None);
        insert_peer_anchor(&store, "2026-11-17", 1, 0x22, "submitted", None);
        insert_peer_anchor(&store, "2026-11-17", 2, 0x33, "submitted", None);

        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(
            report.sets.len(),
            3,
            "one set per anchor, never one per day"
        );
        let chains: Vec<u32> = report.sets.iter().map(|set| set.chain_id).collect();
        assert_eq!(chains, vec![3, 1, 2]);
        assert_eq!(report.sets[0].shares["alpha"], APPCHAIN_ANCHOR_FEE);
        assert!(report.sets[1].shares.is_empty());
        assert!(report.sets[2].shares.is_empty());
        assert_eq!(
            report.totals.cost.total_at(CostBasis::Submitted).base_units,
            3 * APPCHAIN_ANCHOR_FEE
        );
        assert_eq!(report.totals.attributed_base_units, APPCHAIN_ANCHOR_FEE);
    }

    #[test]
    fn every_set_apportions_exactly_its_own_charge_and_nothing_more() {
        let store = store();
        for id in ["alpha", "beta", "gamma"] {
            insert_game(&store, id, "listed");
        }
        // Three apps in a 1:1:1 split on two days: 1000 does not divide evenly by three.
        for date in ["2026-11-16", "2026-11-17"] {
            for id in ["alpha", "beta", "gamma"] {
                insert_results(&store, id, date, 1);
            }
            insert_day(&store, date, "confirmed", None);
            insert_registry_anchor(&store, date, 3, "submitted", None);
        }

        let report = build(&store, "2026-11-16", "2026-11-17", TODAY, Some(3)).unwrap();
        let mut total_shares = 0u64;
        let mut total_fees = 0u64;
        for set in &report.sets {
            assert_eq!(
                set.shares.values().sum::<u64>(),
                set.fee_base_units,
                "set {} {} does not close on its own charge",
                set.chain_id,
                set.day
            );
            total_shares += set.shares.values().sum::<u64>();
            total_fees += set.fee_base_units;
        }
        assert_eq!(total_shares, total_fees);
        assert_eq!(report.totals.attributed_base_units, total_fees);
        // The leftover unit each day goes to the smallest id, so alpha is one unit ahead.
        let alpha = report.apps.iter().find(|app| app.id == "alpha").unwrap();
        let beta = report.apps.iter().find(|app| app.id == "beta").unwrap();
        assert_eq!(alpha.attributed_base_units, 668);
        assert_eq!(beta.attributed_base_units, 666);
    }

    #[test]
    fn the_largest_remainder_rule_closes_inside_the_set() {
        let mut counts = BTreeMap::new();
        counts.insert("a".to_string(), 5u64);
        counts.insert("b".to_string(), 3);
        counts.insert("c".to_string(), 2);
        assert_eq!(allocate(1_000, &counts).values().sum::<u64>(), 1_000);

        // The engineer's example: 1,001 units over 5:3:2 is 500/300/200 plus one leftover unit, which
        // goes to the largest fractional remainder — a's 0.5.
        let shares = allocate(1_001, &counts);
        assert_eq!(shares["a"], 501);
        assert_eq!(shares["b"], 300);
        assert_eq!(shares["c"], 200);
        assert_eq!(shares.values().sum::<u64>(), 1_001);
        assert_eq!(
            allocate(1_001, &counts),
            shares,
            "and the same way every time"
        );
    }

    #[test]
    fn equal_remainders_are_broken_by_ascending_app_id() {
        let mut counts = BTreeMap::new();
        for id in ["alpha", "beta", "gamma"] {
            counts.insert(id.to_string(), 1u64);
        }
        let shares = allocate(1_000, &counts);
        assert_eq!(
            shares["alpha"], 334,
            "the leftover unit goes to the smallest id"
        );
        assert_eq!(shares["beta"], 333);
        assert_eq!(shares["gamma"], 333);
        assert_eq!(allocate(1_000, &counts), shares);
        // Nothing to apportion is not an error, and no unit appears from nowhere.
        assert_eq!(allocate(0, &counts).values().sum::<u64>(), 0);
    }

    #[test]
    fn an_app_with_no_records_still_appears_and_costs_nothing() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_game(&store, "quiet", "registered");
        insert_results(&store, "alpha", "2026-11-17", 1);
        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let quiet = report.apps.iter().find(|app| app.id == "quiet").unwrap();
        assert_eq!((quiet.records, quiet.attributed_base_units), (0, 0));
        assert_eq!(report.totals.apps_without_records, 1);
        assert_eq!(report.totals.apps, 2);
    }

    #[test]
    fn a_past_day_with_records_that_was_never_closed_is_flagged() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-16", 1);
        insert_results(&store, "alpha", "2026-11-18", 1);
        let report = build(&store, "2026-11-16", "2026-11-18", TODAY, Some(3)).unwrap();
        assert!(
            day(&report, "2026-11-16").pending_close,
            "a past day, still open"
        );
        assert!(
            !day(&report, "2026-11-18").pending_close,
            "today is allowed to be open"
        );
        assert_eq!(report.totals.unclosed_past_days, 1);
    }

    #[test]
    fn an_empty_database_reports_zeroes_rather_than_failing() {
        let store = store();
        let report = build(&store, "2026-11-12", "2026-11-18", TODAY, None).unwrap();
        assert_eq!(report.days.len(), 7);
        assert_eq!(report.totals.records, 0);
        assert_eq!(report.totals.cost.total_at(CostBasis::Submitted).base_units, 0);
        assert_eq!(report.totals.cost.total_at(CostBasis::Confirmed).base_units, 0);
        assert!(report.apps.is_empty());
        assert_eq!(report.registry_chain_id, None);
    }

    #[test]
    fn the_same_database_and_window_always_produce_the_same_report() {
        let store = store();
        for id in ["alpha", "beta", "gamma"] {
            insert_game(&store, id, "listed");
            insert_results(&store, id, "2026-11-17", 1);
        }
        insert_day(&store, "2026-11-17", "confirmed", None);
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", None);
        let first = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let second = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn json_and_text_carry_the_same_figures() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 2);
        insert_day(&store, "2026-11-17", "confirmed", Some(&"ff".repeat(32)));
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", Some(&"ff".repeat(32)));

        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let json = report.to_json();
        assert_eq!(
            json["totals"]["cost"]["total_at_basis"]["base_units"],
            json!(report.totals.cost.total_at(report.basis).base_units)
        );
        assert_eq!(json["cost_basis"], json!("submitted"));
        assert_eq!(json["totals"]["basis"], json!("submitted"));
        assert_eq!(json["totals"]["records"], json!(report.totals.records));
        assert_eq!(json["totals"]["cost"]["submitted"]["anchors"], json!(1));
        assert_eq!(json["allocation_sets"][0]["chain_id"], json!(3));
        assert_eq!(json["days"][0]["records"], json!(2));
        assert_eq!(json["apps"][0]["app"], json!("alpha"));

        let text = report.render_text("/tmp/records.db");
        assert!(
            text.contains(&grouped(report.totals.cost.total_at(report.basis).base_units)),
            "{text}"
        );
        assert!(text.contains("read-only"), "{text}");
        assert!(
            text.contains("not a"),
            "the attribution caveat is stated: {text}"
        );
    }

    // --- the cost basis ---------------------------------------------------------------------------

    /// One window holding every class: a confirmed registry anchor, one submitted and not confirmed
    /// here, a peer anchor nothing in this database can confirm, and a reserved slot.
    fn seed_every_class(store: &GamesStore) {
        insert_game(store, "alpha", "listed");
        insert_results(store, "alpha", "2026-11-14", 1);
        insert_results(store, "alpha", "2026-11-15", 2);
        insert_results(store, "alpha", "2026-11-16", 4);
        insert_day(store, "2026-11-14", "confirmed", None);
        insert_registry_anchor(store, "2026-11-14", 3, "submitted", None);
        insert_day(store, "2026-11-15", "anchored", None);
        insert_registry_anchor(store, "2026-11-15", 3, "submitted", None);
        insert_peer_anchor(store, "2026-11-16", 1, 0x51, "submitted", None);
        insert_registry_anchor(store, "2026-11-17", 3, "reserved", None);
    }

    #[test]
    fn the_default_basis_counts_every_accepted_submission() {
        let store = store();
        seed_every_class(&store);

        let report = build(&store, "2026-11-14", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(report.basis, CostBasis::Submitted, "submitted stays the default");

        // The figure the report has always shown: every accepted submission, both chains. The basis
        // added the split; it must not have moved this number.
        let accepted = 3 * APPCHAIN_ANCHOR_FEE;
        assert_eq!(
            report.totals.cost.total_at(CostBasis::Submitted),
            CostTotals {
                anchors: 3,
                base_units: accepted
            }
        );
    }

    #[test]
    fn the_breakdown_closes_in_both_directions() {
        let store = store();
        seed_every_class(&store);
        let cost = build(&store, "2026-11-14", "2026-11-17", TODAY, Some(3))
            .unwrap()
            .totals
            .cost;

        assert_eq!(
            cost.confirmed.plus(cost.awaiting),
            cost.submitted,
            "the registry classes must account for the registry anchors"
        );
        assert_eq!(
            cost.submitted.plus(cost.peer_without_evidence),
            cost.total_at(CostBasis::Submitted)
        );
        assert_eq!(cost.confirmed, cost.total_at(CostBasis::Confirmed));
    }

    #[test]
    fn peer_anchors_are_never_confirmed_and_never_await_evidence() {
        let store = store();
        seed_every_class(&store);

        for basis in [CostBasis::Submitted, CostBasis::Confirmed] {
            let report =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            let cost = report.totals.cost;
            assert_eq!(cost.peer_without_evidence.anchors, 1, "{basis:?}");
            assert_eq!(
                cost.peer_without_evidence.base_units, APPCHAIN_ANCHOR_FEE,
                "{basis:?}"
            );
            // Its money is not silently moved into either registry class.
            assert_eq!(cost.confirmed.base_units, APPCHAIN_ANCHOR_FEE, "{basis:?}");
            assert_eq!(cost.awaiting.base_units, APPCHAIN_ANCHOR_FEE, "{basis:?}");
        }
    }

    #[test]
    fn the_confirmed_basis_counts_only_confirmed_registry_anchors() {
        let store = store();
        seed_every_class(&store);
        let report = build_at(
            &store,
            "2026-11-14",
            "2026-11-17",
            TODAY,
            Some(3),
            CostBasis::Confirmed,
        )
        .unwrap();

        assert_eq!(
            report.totals.cost.total_at(CostBasis::Confirmed).base_units,
            APPCHAIN_ANCHOR_FEE
        );
        // The day whose anchor is only submitted now counts nothing, and says what it is waiting for.
        assert_eq!(
            day(&report, "2026-11-15")
                .cost
                .total_at(CostBasis::Confirmed)
                .base_units,
            0
        );
        assert_eq!(
            day(&report, "2026-11-15").cost.awaiting.base_units,
            APPCHAIN_ANCHOR_FEE
        );
        // So does the peer day, whose charge is still shown rather than dropped.
        assert_eq!(
            day(&report, "2026-11-16")
                .cost
                .total_at(CostBasis::Confirmed)
                .base_units,
            0
        );
        assert_eq!(
            day(&report, "2026-11-16")
                .cost
                .peer_without_evidence
                .base_units,
            APPCHAIN_ANCHOR_FEE
        );
    }

    #[test]
    fn an_all_confirmed_window_reports_the_same_total_in_both_bases() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 3);
        insert_day(&store, "2026-11-17", "confirmed", None);
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", None);

        let submitted =
            build_at(&store, "2026-11-17", "2026-11-17", TODAY, Some(3), CostBasis::Submitted)
                .unwrap();
        let confirmed =
            build_at(&store, "2026-11-17", "2026-11-17", TODAY, Some(3), CostBasis::Confirmed)
                .unwrap();

        assert_eq!(
            submitted.totals.cost.total_at(CostBasis::Submitted),
            confirmed.totals.cost.total_at(CostBasis::Confirmed)
        );
        assert_eq!(submitted.sets, confirmed.sets, "the same anchors form the same sets");
    }

    #[test]
    fn day_rows_sum_to_the_totals_in_every_class() {
        let store = store();
        seed_every_class(&store);

        for basis in [CostBasis::Submitted, CostBasis::Confirmed] {
            let report =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            let mut cost = CostBreakdown::default();
            for row in &report.days {
                cost = cost.plus(row.cost);
            }
            assert_eq!(cost, report.totals.cost, "{basis:?}");
        }
    }

    #[test]
    fn attribution_at_submission_keeps_peer_cost_inside_unattributed() {
        let store = store();
        seed_every_class(&store);
        let report = build(&store, "2026-11-14", "2026-11-17", TODAY, Some(3)).unwrap();

        let counted = report.totals.cost.total_at(CostBasis::Submitted).base_units;
        assert_eq!(
            report.totals.attributed_base_units + report.totals.unattributed_base_units,
            counted,
            "the attribution closes on the total this basis counts"
        );
        assert!(
            report.totals.unattributed_base_units >= APPCHAIN_ANCHOR_FEE,
            "the peer charge sits in the unattributed part, not in an app's share"
        );
    }

    #[test]
    fn attribution_at_confirmation_excludes_peer_and_awaiting() {
        let store = store();
        seed_every_class(&store);
        let report = build_at(
            &store,
            "2026-11-14",
            "2026-11-17",
            TODAY,
            Some(3),
            CostBasis::Confirmed,
        )
        .unwrap();

        let counted = report.totals.cost.total_at(CostBasis::Confirmed).base_units;
        assert_eq!(
            report.totals.attributed_base_units + report.totals.unattributed_base_units,
            counted
        );
        assert_eq!(
            report.sets.len(),
            1,
            "only the confirmed anchor is apportioned at that basis"
        );
        assert_eq!((report.sets[0].chain_id, report.sets[0].day.as_str()), (3, "2026-11-14"));
        assert_eq!(report.totals.anchors_without_records, 0);
    }

    #[test]
    fn every_set_records_the_status_it_was_counted_as() {
        let store = store();
        seed_every_class(&store);

        let submitted = build(&store, "2026-11-14", "2026-11-17", TODAY, Some(3)).unwrap();
        let statuses: Vec<AnchorStatus> = submitted.sets.iter().map(|set| set.status).collect();
        assert_eq!(
            statuses,
            vec![
                AnchorStatus::Confirmed,
                AnchorStatus::Submitted,
                AnchorStatus::Submitted
            ]
        );

        let confirmed =
            build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), CostBasis::Confirmed)
                .unwrap();
        assert!(confirmed
            .sets
            .iter()
            .all(|set| set.status == AnchorStatus::Confirmed));
    }

    #[test]
    fn one_day_with_records_is_called_a_day_and_not_a_days() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 1);

        let one = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let text = one.render_text("/tmp/records.db");
        assert!(text.contains("1 day with records"), "{text}");
        assert!(!text.contains("1 days"), "the plural must not leak: {text}");

        insert_results(&store, "alpha", "2026-11-18", 1);
        let two = build(&store, "2026-11-17", "2026-11-18", TODAY, Some(3)).unwrap();
        let text = two.render_text("/tmp/records.db");
        assert!(text.contains("2 days with records"), "{text}");

        // One app, one day with records, one counted anchor: every noun on that line agrees.
        insert_day(&store, "2026-11-17", "confirmed", None);
        insert_registry_anchor(&store, "2026-11-17", 3, "submitted", None);
        let confirmed = build_at(
            &store,
            "2026-11-17",
            "2026-11-17",
            TODAY,
            Some(3),
            CostBasis::Confirmed,
        )
        .unwrap();
        let text = confirmed.render_text("/tmp/records.db");
        assert!(text.contains("1 app,"), "{text}");
        assert!(text.contains("1 anchor counted at this basis"), "{text}");
        assert!(!text.contains("1 anchors"), "{text}");
    }

    #[test]
    fn the_day_table_headers_do_not_run_together_and_state_the_unit() {
        let store = store();
        insert_game(&store, "alpha", "listed");
        insert_results(&store, "alpha", "2026-11-17", 1);
        let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        let text = report.render_text("/tmp/records.db");

        // Without separators these three labels filled their columns exactly and the header read as a
        // single word: "confirmedawaitingpeer costattributed".
        assert!(text.contains("confirmed awaiting"), "{text}");
        assert!(text.contains("awaiting peer cost"), "{text}");
        assert!(text.contains("peer cost attributed"), "{text}");
        assert!(!text.contains("confirmedawaiting"), "{text}");

        // And the table says what its numbers are, without waiting for the reader to reach the summary.
        assert!(text.contains("1,000,000 base units = 1 ULTRA"), "{text}");
    }

    #[test]
    fn the_text_states_every_class_under_both_bases() {
        let store = store();
        seed_every_class(&store);

        for basis in [CostBasis::Submitted, CostBasis::Confirmed] {
            let report =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            let text = report.render_text("/tmp/records.db");
            for label in [
                "counted at this basis",
                "not counted at this basis",
                "for reference",
                "unsent (reserved or in doubt)",
                "awaiting (registry)",
                "total at submission",
                "total at confirmed",
            ] {
                assert!(text.contains(label), "{basis:?} report lost {label}:\n{text}");
            }
            assert_eq!(
                text.matches("peer_without_evidence").count(),
                1,
                "peer without evidence is stated exactly once, under '{basis:?}'"
            );
        }
    }

    #[test]
    fn json_publishes_the_four_classes_and_the_total_under_both_bases() {
        let store = store();
        seed_every_class(&store);

        for basis in [CostBasis::Submitted, CostBasis::Confirmed] {
            let report =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            let json = report.to_json();
            let cost = &json["totals"]["cost"];
            assert_eq!(
                cost["submitted"]["anchors"],
                json!(report.totals.cost.submitted.anchors)
            );
            assert_eq!(
                cost["confirmed"]["base_units"],
                json!(report.totals.cost.confirmed.base_units)
            );
            assert_eq!(cost["awaiting"]["anchors"], json!(1));
            assert_eq!(cost["peer_without_evidence"]["anchors"], json!(1));
            assert_eq!(
                cost["total_at_basis"]["base_units"],
                json!(report.totals.cost.total_at(basis).base_units)
            );
            assert_eq!(json["cost_basis"], json!(basis.code()));
            assert!(json["days"][0]["cost"]["total_at_basis"].is_object());
        }
    }

    #[test]
    fn the_cost_basis_composes_with_windows_and_json() {
        let parsed =
            parse_args(&argv(&["--days", "3", "--cost-basis", "confirmed", "--json"]), TODAY)
                .unwrap();
        assert_eq!(parsed.cost_basis, CostBasis::Confirmed);
        assert_eq!((parsed.from.as_str(), parsed.to.as_str()), ("2026-11-16", TODAY));
        assert!(parsed.json);

        let default = parse_args(&argv(&["--date", "2026-11-17"]), TODAY).unwrap();
        assert_eq!(
            default.cost_basis,
            CostBasis::Submitted,
            "submitted stays the default"
        );
    }

    #[test]
    fn a_repeated_unknown_or_valueless_cost_basis_is_refused() {
        for values in [
            vec!["--cost-basis", "submitted", "--cost-basis", "confirmed"],
            vec!["--cost-basis", "ledger"],
            vec!["--cost-basis"],
            vec!["--cost-basis", "confirmed", "--cost-basis"],
        ] {
            let error = parse_args(&argv(&values), TODAY)
                .err()
                .unwrap_or_else(|| panic!("{values:?} was accepted, and it should not have been"));
            assert!(error.contains("--cost-basis"), "{values:?} gave: {error}");
        }
        // A typo is named, so the operator can see what was rejected.
        let error = parse_args(&argv(&["--cost-basis", "ledger"]), TODAY).unwrap_err();
        assert!(error.contains("ledger"), "{error}");
    }

    #[test]
    fn the_same_basis_and_window_always_produce_the_same_report() {
        let store = store();
        seed_every_class(&store);

        for basis in [CostBasis::Submitted, CostBasis::Confirmed] {
            let first =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            let second =
                build_at(&store, "2026-11-14", "2026-11-17", TODAY, Some(3), basis).unwrap();
            assert_eq!(first, second, "{basis:?}");
            assert_eq!(first.render_text("db"), second.render_text("db"), "{basis:?}");
            assert_eq!(first.to_json(), second.to_json(), "{basis:?}");
        }
    }

    // --- read-only, on real files -----------------------------------------------------------------

    fn scratch(name: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "records-usage-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("records.db")
    }

    #[test]
    fn a_missing_database_is_an_error_and_is_never_created() {
        let path = scratch("missing");
        let error = match GamesStore::open_read_only(&path) {
            Ok(_) => panic!("a missing database must not open"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("no database"), "{error}");
        assert!(
            !path.exists(),
            "a report must not create the database it is reporting on"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_read_only_connection_refuses_to_write() {
        let path = scratch("writer");
        {
            let writable = GamesStore::open(&path).unwrap();
            insert_game(&writable, "alpha", "listed");
            insert_results(&writable, "alpha", "2026-11-17", 1);
        }
        let read_only = GamesStore::open_read_only(&path).unwrap();
        let report = build(&read_only, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(report.totals.records, 1);

        let connection = read_only.lock().unwrap();
        let refused = connection.execute(
            "INSERT INTO registry_games (id, name, owner, status, key_hash, created_at, updated_at)
             VALUES ('sneaky', 'sneaky', 'x', 'sandbox', x'00', 0, 0)",
            [],
        );
        let error = refused.unwrap_err().to_string().to_ascii_lowercase();
        assert!(error.contains("readonly"), "{error}");
        drop(connection);
        drop(read_only);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// The database a service leaves behind is in WAL mode, and reading it read-only is not free: SQLite
    /// needs the `-wal`/`-shm` companions. This is the case where it cannot have them, because the
    /// directory forbids it. The report must refuse — with an explanation — and must not fall back to a
    /// writable connection to get its numbers, nor leave anything behind.
    #[cfg(unix)]
    #[test]
    fn a_report_refuses_rather_than_opening_writable_when_it_cannot_read_read_only() {
        use std::os::unix::fs::PermissionsExt;

        let path = scratch("wal");
        let dir = path.parent().unwrap().to_path_buf();
        {
            let writable = GamesStore::open(&path).unwrap();
            insert_game(&writable, "alpha", "listed");
            insert_results(&writable, "alpha", "2026-11-17", 3);
        }

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores the mode, so the condition would not be tested at all.
        let still_writable = std::fs::write(dir.join(".probe"), b"x").is_ok();
        let _ = std::fs::remove_file(dir.join(".probe"));
        if still_writable {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        // SQLite needs the WAL companions to read a WAL database read-only. Without them it would have to
        // create them, and in this directory it cannot.
        for companion in ["records.db-wal", "records.db-shm"] {
            let _ = std::fs::remove_file(dir.join(companion));
        }
        let before: Vec<String> = list(&dir);
        let digest = std::fs::read(&path).unwrap();
        let outcome = GamesStore::open_read_only(&path);
        let after = list(&dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        match outcome {
            // This build reads a WAL database read-only without needing to create the companions. Then
            // the figures must still be readable, and the connection must still refuse to write.
            Ok(store) => {
                let report = build(&store, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
                assert_eq!(report.totals.records, 3);
                let refused = store
                    .lock()
                    .unwrap()
                    .execute("DELETE FROM registry_results", []);
                assert!(
                    refused.is_err(),
                    "it opened writable, so it did not read read-only"
                );
            }
            // Or it cannot, and then it must say so rather than open the database another way. This code
            // path has exactly two honest refusals — the schema cannot be verified, or the connection
            // cannot be opened read-only — and either must name the database it is refusing.
            Err(error) => {
                let error = error.to_string();
                assert!(
                    error.contains("records.db"),
                    "the refusal must name the database: {error}"
                );
                assert!(
                    error.contains("schema") || error.contains("read-only"),
                    "the refusal must explain itself: {error}"
                );
            }
        }
        assert_eq!(
            before, after,
            "the report created or removed a file in the directory"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            digest,
            "the database was modified"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// And here it can have them, which is the normal case: the directory is writable. SQLite will
    /// create `-wal`/`-shm`, but the database file itself must come out byte for byte unchanged.
    #[test]
    fn a_report_never_changes_the_database_file() {
        let path = scratch("unchanged");
        {
            let writable = GamesStore::open(&path).unwrap();
            insert_game(&writable, "alpha", "listed");
            insert_results(&writable, "alpha", "2026-11-17", 3);
        }
        let digest = std::fs::read(&path).unwrap();

        let read_only = GamesStore::open_read_only(&path).unwrap();
        let report = build(&read_only, "2026-11-17", "2026-11-17", TODAY, Some(3)).unwrap();
        assert_eq!(report.totals.records, 3);
        drop(read_only);

        assert_eq!(
            std::fs::read(&path).unwrap(),
            digest,
            "reading the report changed the database file"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[cfg(unix)]
    fn list(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    // --- the schema gate ---------------------------------------------------------------------------

    fn read_only_error(path: &Path) -> String {
        match GamesStore::open_read_only(path) {
            Ok(_) => panic!("{} opened, and should not have", path.display()),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_database_from_another_schema_is_refused_rather_than_misread() {
        let path = scratch("schema3");
        {
            let writable = GamesStore::open(&path).unwrap();
            insert_game(&writable, "alpha", "listed");
            writable
                .lock()
                .unwrap()
                .execute("UPDATE meta SET value = '3' WHERE key = 'schema_version'", [])
                .unwrap();
        }
        let error = read_only_error(&path);
        assert!(error.contains("schema 3"), "{error}");
        assert!(error.contains("needs schema 8"), "{error}");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_database_without_a_schema_marker_is_refused_as_unknown() {
        let path = scratch("noschema");
        {
            let writable = GamesStore::open(&path).unwrap();
            insert_game(&writable, "alpha", "listed");
            writable.lock().unwrap().execute("DELETE FROM meta", []).unwrap();
        }
        let error = read_only_error(&path);
        assert!(error.contains("unknown"), "{error}");
        assert!(error.contains("needs schema 8"), "{error}");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    // --- the arguments -----------------------------------------------------------------------------

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn the_default_window_is_the_seven_days_ending_today() {
        let parsed = parse_args(&argv(&[]), TODAY).unwrap();
        assert_eq!((parsed.from.as_str(), parsed.to.as_str()), ("2026-11-12", TODAY));
        assert!(!parsed.json);
    }

    #[test]
    fn a_single_day_a_range_a_count_and_an_open_end_all_parse() {
        let one = parse_args(&argv(&["--date", "2026-11-17"]), TODAY).unwrap();
        assert_eq!((one.from.as_str(), one.to.as_str()), ("2026-11-17", "2026-11-17"));

        let range =
            parse_args(&argv(&["--from", "2026-11-01", "--to", "2026-11-30"]), TODAY).unwrap();
        assert_eq!((range.from.as_str(), range.to.as_str()), ("2026-11-01", "2026-11-30"));

        // --to alone keeps the default width, ending there.
        let end = parse_args(&argv(&["--to", "2026-11-17"]), TODAY).unwrap();
        assert_eq!((end.from.as_str(), end.to.as_str()), ("2026-11-11", "2026-11-17"));

        // --from alone runs to today.
        let open = parse_args(&argv(&["--from", "2026-11-01"]), TODAY).unwrap();
        assert_eq!((open.from.as_str(), open.to.as_str()), ("2026-11-01", TODAY));

        let counted = parse_args(&argv(&["--days", "30"]), TODAY).unwrap();
        assert_eq!((counted.from.as_str(), counted.to.as_str()), ("2026-10-20", TODAY));

        let counted_back = parse_args(&argv(&["--days", "3", "--to", "2026-11-17"]), TODAY).unwrap();
        assert_eq!((counted_back.from.as_str(), counted_back.to.as_str()), ("2026-11-15", "2026-11-17"));
    }

    #[test]
    fn json_composes_with_every_window() {
        for values in [
            vec!["--json"],
            vec!["--json", "--date", "2026-11-17"],
            vec!["--json", "--from", "2026-11-01", "--to", "2026-11-02"],
            vec!["--json", "--days", "3"],
        ] {
            let parsed = parse_args(&argv(&values), TODAY)
                .unwrap_or_else(|error| panic!("{values:?} was refused: {error}"));
            assert!(parsed.json, "{values:?} lost --json");
        }
    }

    #[test]
    fn contradictory_windows_are_refused_rather_than_silently_resolved() {
        for (values, expected) in [
            (vec!["--date", "2026-11-17", "--from", "2026-11-01"], "--date"),
            (vec!["--date", "2026-11-17", "--to", "2026-11-18"], "--date"),
            (vec!["--date", "2026-11-17", "--days", "3"], "--date"),
            (vec!["--days", "3", "--from", "2026-11-01"], "--days"),
        ] {
            let error = parse_args(&argv(&values), TODAY)
                .err()
                .unwrap_or_else(|| panic!("{values:?} was accepted, and it should not have been"));
            assert!(error.contains(expected), "{values:?} gave: {error}");
        }
    }

    #[test]
    fn unknown_flags_and_missing_or_bad_values_are_refused_by_name() {
        for (values, expected) in [
            (vec!["--jsno"], "--jsno"),
            (vec!["--date"], "--date"),
            (vec!["--from"], "--from"),
            (vec!["--days", "0"], "--days"),
            (vec!["--days", "seven"], "--days"),
            (vec!["--date", "2026-11-17", "--loud"], "--loud"),
        ] {
            let error = parse_args(&argv(&values), TODAY)
                .err()
                .unwrap_or_else(|| panic!("{values:?} was accepted, and it should not have been"));
            assert!(error.contains(expected), "{values:?} gave: {error}");
        }
    }

    #[test]
    fn a_repeated_flag_is_refused_instead_of_letting_the_last_one_win() {
        for given in [
            vec!["--date", "2026-11-16", "--date", "2026-11-17"],
            vec!["--from", "2026-11-01", "--from", "2026-11-02"],
            vec!["--to", "2026-11-17", "--to", "2026-11-18"],
            vec!["--days", "3", "--days", "4"],
            vec!["--json", "--json"],
        ] {
            let error = parse_args(&argv(&given), TODAY)
                .err()
                .unwrap_or_else(|| panic!("{given:?} was accepted, and should not have been"));
            assert!(error.contains("more than once"), "{given:?} gave: {error}");
        }
    }

    #[test]
    fn counting_back_crosses_months_and_leap_days() {
        assert_eq!(days_before("2026-03-01", 1).as_deref(), Some("2026-02-28"));
        assert_eq!(days_before("2028-03-01", 1).as_deref(), Some("2028-02-29"));
        assert_eq!(days_before("2026-01-01", 1).as_deref(), Some("2025-12-31"));
        assert_eq!(days_before("2026-11-18", 0).as_deref(), Some("2026-11-18"));
        assert_eq!(days_before("not-a-date", 1), None);
    }
}
