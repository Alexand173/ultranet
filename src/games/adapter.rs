//! A deterministic WebAssembly runtime for game adapters.
//!
//! An adapter is a small WebAssembly module that judges one game result: given the player and the game's own
//! result bytes, it says whether that result is a win, a loss, a tie, a cancellation or invalid, and by how
//! much. This module runs such a module **and trusts nothing it says about itself**. It does not decide who
//! may register an adapter, which module a game uses or whether a result earns money; those are later stages
//! and none of them is connected here. Running an adapter pays nobody and touches no ledger or database.
//!
//! **Why WebAssembly alone is not enough.** A module runs inside a sandbox, but a sandbox is not a guarantee
//! of determinism and not a guarantee of safety. These rules are what the host adds, each with a test:
//!
//! * **No host access of any kind.** A module may import nothing, so it has no clock, no network, no file, no
//!   randomness and no way to call back into the host. A module with an import is refused at load.
//! * **No floating point.** NaN bit patterns are the one thing the WebAssembly specification leaves open, so
//!   floats are switched off and a module that uses them is refused by validation. SIMD, threads, reference
//!   types, tail calls, multiple memories, multiple values, saturating float-to-int, extended const and 64-bit
//!   memory are off too: the instruction set is the integer core plus mutable globals, sign extension and bulk
//!   memory.
//! * **Bulk memory is allowed, and metered per byte.** `memory.copy` and `memory.fill` were refused once, until a
//!   real module was measured: every Rust-built adapter needs them, because `memcpy` and `memset` ship
//!   precompiled in the wasm standard library and are compiled with bulk memory on, so nothing we do at build
//!   time removes them. They are safe to admit here because they touch only the module's own linear memory (which
//!   is capped at [`RuntimeLimits::max_memory_pages`]) and because the runtime charges fuel per byte copied or
//!   filled, not one unit per instruction — the tests measure that ratio against the length, so a bulk operation
//!   cannot buy work the equivalent loop would have paid for.
//! * **Fuel.** Every call gets a fixed amount of fuel; an infinite loop is a trap, never a hang, and the fuel a
//!   run used is part of the result, so two runs that differ in work are caught.
//! * **Memory.** The module must declare a maximum and it must not exceed the host's page limit; growth past
//!   the limit fails identically on every run. The value stack and call depth are bounded.
//! * **A fresh instance for every run.** Nothing survives from one run to the next: no global, no memory.
//! * **Bounded input and output**, both of exact, versioned layouts (below). A module cannot make the host
//!   read outside its own memory.
//! * **Identity.** A module is identified by the SHA3-256 of its exact bytes. A different byte is a different
//!   module; "upgrading" an adapter means a new hash, never a change to an old one.
//!
//! **A module cannot declare its result valid.** The output has no "verified" field. Whether a result counts
//! is the host's decision, made from the output *and* the checks above *and* (later) the authorisation of the
//! module's hash. [`AdapterModule::validate_result`] runs the module twice on fresh instances and refuses
//! unless both runs agree on every output byte and on the fuel used.
//!
//! # Adapter ABI, version 1
//!
//! The module exports, with exactly these types:
//!
//! ```text
//! memory                        the one linear memory, with a declared maximum
//! adapter_abi_version() -> i32  must return 1
//! input_buffer()        -> i32  where the host writes the input
//! input_capacity()      -> i32  how many bytes the input region holds
//! output_buffer()       -> i32  where the module writes the output
//! adapter_run(len: i32) -> i32  runs; returns the output length (must be 38) or a negative refusal code
//! ```
//!
//! Input, `len` bytes at `input_buffer()`:
//!
//! ```text
//! u8 abi_version (1) || 32-byte player address || u32le payload_len || payload (the game's own bytes)
//! ```
//!
//! Output, 38 bytes at `output_buffer()`:
//!
//! ```text
//! u8 abi_version (1) || u8 outcome (0 invalid, 1 win, 2 loss, 3 tie, 4 cancelled)
//!   || u32le score || 32-byte result digest (the module's own commitment to what it judged)
//! ```
//!
//! Versioning: a host that implements ABI 1 runs only modules whose `adapter_abi_version()` is 1. A future
//! ABI is a different number and a different host path; it never reinterprets these bytes.

use sha3::{Digest, Sha3_256};
use wasmi::{
    TrapCode, Config, Engine, EnforcedLimits, ExternType, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, ValType,
};

pub const ABI_VERSION: u8 = 1;
pub const OUTPUT_BYTES: usize = 38;
const WASM_PAGE_BYTES: usize = 65_536;

const EXPORT_MEMORY: &str = "memory";
const EXPORT_ABI: &str = "adapter_abi_version";
const EXPORT_INPUT: &str = "input_buffer";
const EXPORT_CAPACITY: &str = "input_capacity";
const EXPORT_OUTPUT: &str = "output_buffer";
const EXPORT_RUN: &str = "adapter_run";

/// What the host allows a module to use. Every limit is checked, none is advisory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLimits {
    /// The largest module the host will load.
    pub max_module_bytes: usize,
    /// The most linear-memory pages (64 KiB each) a module may declare as its maximum.
    pub max_memory_pages: u32,
    /// Fuel for one call; an adapter that needs more traps with `OutOfFuel`.
    pub fuel_per_run: u64,
    /// The largest payload the host will pass in.
    pub max_payload_bytes: usize,
    /// How deep calls may nest before the run traps.
    pub max_call_depth: usize,
}

impl Default for RuntimeLimits {
    fn default() -> RuntimeLimits {
        RuntimeLimits {
            max_module_bytes: 256 * 1024,
            max_memory_pages: 16,
            fuel_per_run: 5_000_000,
            max_payload_bytes: 4_096,
            max_call_depth: 128,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdapterError {
    #[error("the module is larger than the {limit} bytes this host loads")]
    ModuleTooLarge { limit: usize },
    #[error("the module is not valid under the adapter rules: {0}")]
    Invalid(String),
    #[error("the module imports {0}; adapters may import nothing")]
    ForbiddenImport(String),
    #[error("the module does not export `{0}`")]
    MissingExport(&'static str),
    #[error("the export `{0}` has the wrong type")]
    BadExportType(&'static str),
    #[error("the module's memory is not acceptable: {0}")]
    MemoryDeclaration(String),
    #[error("the module speaks adapter ABI {found}; this host runs ABI {ABI_VERSION}")]
    AbiVersion { found: i32 },
    #[error("the input is larger than this module or this host accepts")]
    InputTooLarge,
    #[error("the module could not be started: {0}")]
    Instantiate(String),
    #[error("the module ran out of fuel")]
    OutOfFuel,
    #[error("the module trapped: {0}")]
    Trap(String),
    #[error("the module refused the input with code {0}")]
    GuestRefused(i32),
    #[error("the module produced an unusable output: {0}")]
    BadOutput(String),
    #[error("two runs of the same module on the same input disagreed")]
    Nondeterministic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Invalid = 0,
    Win = 1,
    Loss = 2,
    Tie = 3,
    Cancelled = 4,
}

impl Outcome {
    fn from_code(code: u8) -> Option<Outcome> {
        Some(match code {
            0 => Outcome::Invalid,
            1 => Outcome::Win,
            2 => Outcome::Loss,
            3 => Outcome::Tie,
            4 => Outcome::Cancelled,
            _ => return None,
        })
    }
}

/// What a module said about one input. Note what is missing: any way for the module to say it is verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterOutput {
    pub outcome: Outcome,
    pub score: u32,
    pub result_digest: [u8; 32],
}

/// One execution: the output and the work it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub output: AdapterOutput,
    pub raw_output: Vec<u8>,
    pub fuel_used: u64,
}

/// A result the host will stand behind: two fresh runs agreed on every byte and on the fuel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedResult {
    pub output: AdapterOutput,
    pub fuel_used: u64,
    /// The SHA3-256 of the module that produced it.
    pub module_hash: [u8; 32],
}

/// A module that passed every structural check, ready to run.
pub struct AdapterModule {
    engine: Engine,
    module: Module,
    hash: [u8; 32],
    limits: RuntimeLimits,
}

fn engine_for(limits: &RuntimeLimits) -> Engine {
    let mut config = Config::default();
    config
        .consume_fuel(true)
        .floats(false)
        .wasm_mutable_global(true)
        .wasm_sign_extension(true)
        .wasm_saturating_float_to_int(false)
        .wasm_multi_value(false)
        .wasm_multi_memory(false)
        // The one extension that is on: `memory.copy` and `memory.fill` only, never tables (`table.copy`
        // needs reference types, which stay off). See the module docs — this is allowed because wasmi
        // meters it per byte and it reaches nothing but the module's own capped linear memory.
        .wasm_bulk_memory(true)
        .wasm_reference_types(false)
        .wasm_tail_call(false)
        .wasm_extended_const(false)
        .wasm_custom_page_sizes(false)
        .wasm_wide_arithmetic(false)
        .ignore_custom_sections(true)
        .compilation_mode(wasmi::CompilationMode::Eager)
        .enforced_limits(EnforcedLimits::strict())
        .set_max_recursion_depth(limits.max_call_depth);
    Engine::new(&config)
}

fn is_i32(types: &[ValType]) -> bool {
    types == [ValType::I32]
}

impl AdapterModule {
    /// Load and check a module. Nothing runs here: this is structure only.
    pub fn compile(bytes: &[u8], limits: RuntimeLimits) -> Result<AdapterModule, AdapterError> {
        if bytes.len() > limits.max_module_bytes {
            return Err(AdapterError::ModuleTooLarge { limit: limits.max_module_bytes });
        }
        let engine = engine_for(&limits);
        // Module::new validates the whole module under the features above, floats included.
        let module = Module::new(&engine, bytes).map_err(|error| AdapterError::Invalid(error.to_string()))?;
        if let Some(import) = module.imports().next() {
            return Err(AdapterError::ForbiddenImport(format!("{}.{}", import.module(), import.name())));
        }
        let mut memory_checked = false;
        let mut seen = [false; 5];
        for export in module.exports() {
            match (export.name(), export.ty()) {
                (EXPORT_MEMORY, ExternType::Memory(memory)) => {
                    if memory.is_64() {
                        return Err(AdapterError::MemoryDeclaration("64-bit memory is not allowed".into()));
                    }
                    let Some(max) = memory.maximum() else {
                        return Err(AdapterError::MemoryDeclaration("a maximum size must be declared".into()));
                    };
                    if max > u64::from(limits.max_memory_pages) {
                        return Err(AdapterError::MemoryDeclaration(format!(
                            "the declared maximum of {max} pages exceeds the host's {}",
                            limits.max_memory_pages
                        )));
                    }
                    if memory.minimum() > max {
                        return Err(AdapterError::MemoryDeclaration("the minimum exceeds the maximum".into()));
                    }
                    memory_checked = true;
                }
                (EXPORT_MEMORY, _) => return Err(AdapterError::BadExportType(EXPORT_MEMORY)),
                (EXPORT_ABI, ExternType::Func(f)) if f.params().is_empty() && is_i32(f.results()) => seen[0] = true,
                (EXPORT_INPUT, ExternType::Func(f)) if f.params().is_empty() && is_i32(f.results()) => seen[1] = true,
                (EXPORT_CAPACITY, ExternType::Func(f)) if f.params().is_empty() && is_i32(f.results()) => seen[2] = true,
                (EXPORT_OUTPUT, ExternType::Func(f)) if f.params().is_empty() && is_i32(f.results()) => seen[3] = true,
                (EXPORT_RUN, ExternType::Func(f)) if is_i32(f.params()) && is_i32(f.results()) => seen[4] = true,
                (name @ (EXPORT_ABI | EXPORT_INPUT | EXPORT_CAPACITY | EXPORT_OUTPUT | EXPORT_RUN), _) => {
                    let name = [EXPORT_ABI, EXPORT_INPUT, EXPORT_CAPACITY, EXPORT_OUTPUT, EXPORT_RUN]
                        .into_iter()
                        .find(|candidate| *candidate == name)
                        .unwrap_or(EXPORT_RUN);
                    return Err(AdapterError::BadExportType(name));
                }
                _ => {} // other exports are ignored: the host never calls them
            }
        }
        if !memory_checked {
            return Err(AdapterError::MissingExport(EXPORT_MEMORY));
        }
        for (present, name) in seen.into_iter().zip([EXPORT_ABI, EXPORT_INPUT, EXPORT_CAPACITY, EXPORT_OUTPUT, EXPORT_RUN]) {
            if !present {
                return Err(AdapterError::MissingExport(name));
            }
        }
        Ok(AdapterModule { engine, module, hash: Sha3_256::digest(bytes).into(), limits })
    }

    /// The identity of this exact module.
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    /// Run once on a fresh instance. A trap, fuel exhaustion or a malformed output is an error.
    pub fn run_once(&self, player: &[u8; 32], payload: &[u8]) -> Result<RunRecord, AdapterError> {
        if payload.len() > self.limits.max_payload_bytes {
            return Err(AdapterError::InputTooLarge);
        }
        let memory_bytes = self.limits.max_memory_pages as usize * WASM_PAGE_BYTES;
        let store_limits: StoreLimits = StoreLimitsBuilder::new()
            .memory_size(memory_bytes)
            .instances(1)
            .memories(1)
            .tables(1)
            .build();
        let mut store = Store::new(&self.engine, store_limits);
        store.limiter(|limits| limits);
        store.set_fuel(self.limits.fuel_per_run).map_err(|e| AdapterError::Instantiate(e.to_string()))?;
        let linker = <Linker<StoreLimits>>::new(&self.engine);
        // A start function, if the module has one, runs here, under the same fuel.
        let instance = linker
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|error| classify(&error, true))?;

        let call_i32 = |store: &mut Store<StoreLimits>, name: &'static str| -> Result<i32, AdapterError> {
            let function = instance
                .get_typed_func::<(), i32>(&*store, name)
                .map_err(|_| AdapterError::BadExportType(name))?;
            function.call(&mut *store, ()).map_err(|error| classify(&error, false))
        };
        let version = call_i32(&mut store, EXPORT_ABI)?;
        if version != i32::from(ABI_VERSION) {
            return Err(AdapterError::AbiVersion { found: version });
        }
        let input_ptr = call_i32(&mut store, EXPORT_INPUT)?;
        let capacity = call_i32(&mut store, EXPORT_CAPACITY)?;
        let output_ptr = call_i32(&mut store, EXPORT_OUTPUT)?;
        let memory = instance
            .get_memory(&store, EXPORT_MEMORY)
            .ok_or(AdapterError::MissingExport(EXPORT_MEMORY))?;
        let memory_len = memory.data_size(&store);

        // The input: version, player, payload length, payload.
        let mut input = Vec::with_capacity(37 + payload.len());
        input.push(ABI_VERSION);
        input.extend_from_slice(player);
        input.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        input.extend_from_slice(payload);
        let (input_ptr, capacity) = (to_offset(input_ptr)?, to_offset(capacity)?);
        if input.len() > capacity {
            return Err(AdapterError::InputTooLarge);
        }
        if input_ptr.checked_add(capacity).is_none_or(|end| end > memory_len) {
            return Err(AdapterError::BadOutput("the input region is outside the module's memory".into()));
        }
        memory
            .write(&mut store, input_ptr, &input)
            .map_err(|_| AdapterError::BadOutput("the input could not be written".into()))?;

        let run = instance
            .get_typed_func::<i32, i32>(&store, EXPORT_RUN)
            .map_err(|_| AdapterError::BadExportType(EXPORT_RUN))?;
        let length = run.call(&mut store, input.len() as i32).map_err(|error| classify(&error, false))?;
        if length < 0 {
            return Err(AdapterError::GuestRefused(length));
        }
        if length as usize != OUTPUT_BYTES {
            return Err(AdapterError::BadOutput(format!("the output is {length} bytes, not {OUTPUT_BYTES}")));
        }
        let output_ptr = to_offset(output_ptr)?;
        if output_ptr.checked_add(OUTPUT_BYTES).is_none_or(|end| end > memory_len) {
            return Err(AdapterError::BadOutput("the output region is outside the module's memory".into()));
        }
        let mut raw = vec![0u8; OUTPUT_BYTES];
        memory
            .read(&store, output_ptr, &mut raw)
            .map_err(|_| AdapterError::BadOutput("the output could not be read".into()))?;
        let output = parse_output(&raw)?;
        let remaining = store.get_fuel().map_err(|e| AdapterError::Instantiate(e.to_string()))?;
        Ok(RunRecord { output, raw_output: raw, fuel_used: self.limits.fuel_per_run - remaining })
    }

    /// The only way to obtain a result the host will stand behind: two fresh runs, which must agree.
    pub fn validate_result(&self, player: &[u8; 32], payload: &[u8]) -> Result<ValidatedResult, AdapterError> {
        validate_with(self.hash, || self.run_once(player, payload))
    }
}

/// Run twice and require agreement. Separate from [`AdapterModule`] so that the rule itself, and not only
/// the runs, can be tested: a real module cannot be made to disagree with itself, which is the point.
fn validate_with<F>(module_hash: [u8; 32], mut run: F) -> Result<ValidatedResult, AdapterError>
where
    F: FnMut() -> Result<RunRecord, AdapterError>,
{
    let first = run()?;
    let second = run()?;
    compare_runs(&first, &second)?;
    Ok(ValidatedResult { output: first.output, fuel_used: first.fuel_used, module_hash })
}

/// Two runs of one module on one input must agree on every output byte and on the fuel they used.
pub fn compare_runs(first: &RunRecord, second: &RunRecord) -> Result<(), AdapterError> {
    if first.raw_output != second.raw_output || first.fuel_used != second.fuel_used {
        return Err(AdapterError::Nondeterministic);
    }
    Ok(())
}

fn to_offset(value: i32) -> Result<usize, AdapterError> {
    usize::try_from(value).map_err(|_| AdapterError::BadOutput("a buffer address is negative".into()))
}

fn parse_output(raw: &[u8]) -> Result<AdapterOutput, AdapterError> {
    if raw.len() != OUTPUT_BYTES {
        return Err(AdapterError::BadOutput("wrong length".into()));
    }
    if raw[0] != ABI_VERSION {
        return Err(AdapterError::BadOutput(format!("the output declares ABI {}", raw[0])));
    }
    let outcome = Outcome::from_code(raw[1]).ok_or_else(|| AdapterError::BadOutput(format!("unknown outcome {}", raw[1])))?;
    let score = u32::from_le_bytes([raw[2], raw[3], raw[4], raw[5]]);
    let mut result_digest = [0u8; 32];
    result_digest.copy_from_slice(&raw[6..38]);
    Ok(AdapterOutput { outcome, score, result_digest })
}

fn classify(error: &wasmi::Error, while_starting: bool) -> AdapterError {
    match error.as_trap_code() {
        Some(TrapCode::OutOfFuel) => AdapterError::OutOfFuel,
        Some(code) => AdapterError::Trap(code.to_string()),
        None if while_starting => AdapterError::Instantiate(error.to_string()),
        None => AdapterError::Trap(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: [u8; 32] = [7; 32];

    /// A complete, well-behaved adapter. The payload's first byte picks the outcome (1 win, 2 loss, 3 tie,
    /// 4 cancelled, anything else invalid), the score is the sum of the payload bytes, and the digest is the
    /// player's address, so a test can see that the input really reached the module.
    const GOOD: &str = r#"
    (module
      (memory (export "memory") 1 2)
      (global $runs (mut i32) (i32.const 0))
      (func (export "adapter_abi_version") (result i32) i32.const 1)
      (func (export "input_buffer") (result i32) i32.const 1024)
      (func (export "input_capacity") (result i32) i32.const 4200)
      (func (export "output_buffer") (result i32) i32.const 8192)
      (func (export "adapter_run") (param $len i32) (result i32)
        (local $i i32) (local $sum i32) (local $first i32) (local $plen i32)
        (global.set $runs (i32.add (global.get $runs) (i32.const 1)))
        (local.set $plen (i32.load (i32.const 1057)))
        (block $done (loop $l
          (br_if $done (i32.ge_u (local.get $i) (local.get $plen)))
          (local.set $sum (i32.add (local.get $sum) (i32.load8_u (i32.add (i32.const 1061) (local.get $i)))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $l)))
        (i32.store8 (i32.const 8192) (i32.const 1))
        (if (i32.gt_u (local.get $plen) (i32.const 0))
          (then (local.set $first (i32.load8_u (i32.const 1061)))))
        (i32.store8 (i32.const 8193)
          (select (local.get $first) (i32.const 0) (i32.le_u (local.get $first) (i32.const 4))))
        ;; the score also carries how many times this instance has run: always 1, because every run is fresh
        (i32.store (i32.const 8194) (i32.add (local.get $sum) (i32.mul (global.get $runs) (i32.const 1000000))))
        (local.set $i (i32.const 0))
        (block $d (loop $c
          (br_if $d (i32.ge_u (local.get $i) (i32.const 32)))
          (i32.store8 (i32.add (i32.const 8198) (local.get $i))
                      (i32.load8_u (i32.add (i32.const 1025) (local.get $i))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $c)))
        (i32.const 38)))
    "#;

    fn compile_with(wat: &str, limits: RuntimeLimits) -> Result<AdapterModule, AdapterError> {
        let bytes = ::wat::parse_str(wat).expect("the test module is valid text");
        AdapterModule::compile(&bytes, limits)
    }

    fn compile(wat: &str) -> Result<AdapterModule, AdapterError> {
        compile_with(wat, RuntimeLimits::default())
    }

    /// The boilerplate every test module needs, with the body of `adapter_run` and extra items supplied.
    fn adapter(extra: &str, body: &str) -> String {
        adapter_with_imports("", extra, body)
    }

    /// The same, with imports (which WebAssembly text requires to come before everything else).
    fn adapter_with_imports(imports: &str, extra: &str, body: &str) -> String {
        format!(
            r#"(module
              {imports}
              (memory (export "memory") 1 2)
              {extra}
              (func (export "adapter_abi_version") (result i32) i32.const 1)
              (func (export "input_buffer") (result i32) i32.const 1024)
              (func (export "input_capacity") (result i32) i32.const 4200)
              (func (export "output_buffer") (result i32) i32.const 8192)
              (func (export "adapter_run") (param $len i32) (result i32) {body}))"#
        )
    }

    const WRITE_VALID_OUTPUT: &str = "(i32.store8 (i32.const 8192) (i32.const 1)) (i32.store8 (i32.const 8193) (i32.const 1))";

    // --- a good adapter ----------------------------------------------------------------------

    #[test]
    fn a_well_behaved_adapter_judges_a_result_and_reads_its_input() {
        let module = compile(GOOD).unwrap();
        let win = module.validate_result(&PLAYER, &[1, 10, 20]).unwrap();
        assert_eq!(win.output.outcome, Outcome::Win);
        assert_eq!(win.output.score, 31 + 1_000_000, "the payload was summed; the run counter is 1");
        assert_eq!(win.output.result_digest, PLAYER, "the player's address reached the module");
        assert_eq!(win.module_hash, module.hash());
        assert!(win.fuel_used > 0);
        for (code, outcome) in [(2u8, Outcome::Loss), (3, Outcome::Tie), (4, Outcome::Cancelled), (9, Outcome::Invalid)] {
            assert_eq!(module.validate_result(&PLAYER, &[code]).unwrap().output.outcome, outcome);
        }
        assert_eq!(module.validate_result(&PLAYER, &[]).unwrap().output.outcome, Outcome::Invalid, "an empty payload is fine");
    }

    #[test]
    fn the_same_input_gives_the_same_result_and_the_same_fuel_every_time() {
        let module = compile(GOOD).unwrap();
        let first = module.run_once(&PLAYER, &[1, 2, 3, 4]).unwrap();
        for _ in 0..5 {
            assert_eq!(module.run_once(&PLAYER, &[1, 2, 3, 4]).unwrap(), first);
        }
        // A different input is a different result, and a longer payload costs more fuel.
        assert_ne!(module.run_once(&PLAYER, &[1, 2, 3, 5]).unwrap().raw_output, first.raw_output);
        assert!(module.run_once(&PLAYER, &[1; 200]).unwrap().fuel_used > first.fuel_used);
        // Two separately compiled copies of the same bytes agree with each other and share a hash.
        let again = compile(GOOD).unwrap();
        assert_eq!(again.hash(), module.hash());
        assert_eq!(again.run_once(&PLAYER, &[1, 2, 3, 4]).unwrap(), first);
    }

    #[test]
    fn nothing_survives_from_one_run_to_the_next() {
        // `GOOD` counts its own runs in a mutable global. If an instance were reused the score would grow.
        let module = compile(GOOD).unwrap();
        for _ in 0..4 {
            assert_eq!(module.run_once(&PLAYER, &[1]).unwrap().output.score, 1 + 1_000_000);
        }
    }

    #[test]
    fn two_runs_that_disagree_are_refused_as_nondeterministic() {
        let module = compile(GOOD).unwrap();
        let record = module.run_once(&PLAYER, &[1]).unwrap();
        assert!(compare_runs(&record, &record.clone()).is_ok());
        let mut other_output = record.clone();
        other_output.raw_output[2] ^= 1;
        assert_eq!(compare_runs(&record, &other_output), Err(AdapterError::Nondeterministic));
        let mut other_work = record.clone();
        other_work.fuel_used += 1;
        assert_eq!(compare_runs(&record, &other_work), Err(AdapterError::Nondeterministic), "the same answer by different work is also refused");
    }

    // --- what a module may not be --------------------------------------------------------------

    #[test]
    fn a_module_that_imports_anything_is_refused_so_it_has_no_clock_network_or_randomness() {
        for import in [
            r#"(import "env" "now" (func (result i64)))"#,
            r#"(import "wasi_snapshot_preview1" "clock_time_get" (func (param i32 i64 i32) (result i32)))"#,
            r#"(import "env" "random" (func (result i32)))"#,
            r#"(import "env" "table" (table 1 funcref))"#,
            r#"(import "env" "counter" (global (mut i32)))"#,
        ] {
            let wat = adapter_with_imports(import, "", &format!("{WRITE_VALID_OUTPUT} i32.const 38"));
            assert!(matches!(compile(&wat), Err(AdapterError::ForbiddenImport(_)) | Err(AdapterError::Invalid(_))), "{import}");
        }
        // The plain case has a precise answer: the import is named.
        let wat = adapter_with_imports(r#"(import "env" "now" (func (result i64)))"#, "", &format!("{WRITE_VALID_OUTPUT} i32.const 38"));
        assert_eq!(compile(&wat).err(), Some(AdapterError::ForbiddenImport("env.now".into())));
    }

    #[test]
    fn floating_point_and_other_extensions_are_refused_by_validation() {
        let float = adapter("", &format!("{WRITE_VALID_OUTPUT} (drop (f32.add (f32.const 1) (f32.const 2))) i32.const 38"));
        assert!(matches!(compile(&float), Err(AdapterError::Invalid(_))), "f32");
        let double = adapter("(global $g f64 (f64.const 0))", &format!("{WRITE_VALID_OUTPUT} i32.const 38"));
        assert!(matches!(compile(&double), Err(AdapterError::Invalid(_))), "an f64 global");
        let simd = adapter("", &format!("{WRITE_VALID_OUTPUT} (drop (v128.const i32x4 0 0 0 0)) i32.const 38"));
        assert!(matches!(compile(&simd), Err(AdapterError::Invalid(_))), "simd");
        let memory64 = r#"(module (memory (export "memory") i64 1 2))"#;
        assert!(matches!(compile(memory64), Err(AdapterError::Invalid(_))), "64-bit memory");
        // Saturating float-to-int needs an f32 operand, so with floats off it is refused twice over; it is
        // listed rather than dropped because an adapter author does try it.
        let saturating = adapter("", "(drop (i32.trunc_sat_f32_s (f32.const 1))) i32.const 38");
        assert!(matches!(compile(&saturating), Err(AdapterError::Invalid(_))), "i32.trunc_sat_f32_s");
    }

    /// Admitting bulk memory must not have admitted anything else. Each of these was refused before the
    /// change and is refused now, so the instruction set grew by exactly one family.
    #[test]
    fn bulk_memory_did_not_bring_any_other_extension_with_it() {
        let multi_value = adapter(
            "(func $two (result i32 i32) (i32.const 1) (i32.const 2))",
            &format!("{WRITE_VALID_OUTPUT} (call $two) (drop) (drop) i32.const 38"),
        );
        assert!(matches!(compile(&multi_value), Err(AdapterError::Invalid(_))), "multi-value");

        let reference_types = adapter(
            "(func $nullable (result i32) (ref.is_null (ref.null func)))",
            &format!("{WRITE_VALID_OUTPUT} (drop (call $nullable)) i32.const 38"),
        );
        assert!(matches!(compile(&reference_types), Err(AdapterError::Invalid(_))), "reference types");

        let tail_call = adapter(
            "(func $tail (result i32) (i32.const 7))",
            &format!("{WRITE_VALID_OUTPUT} (return_call $tail)"),
        );
        assert!(matches!(compile(&tail_call), Err(AdapterError::Invalid(_))), "tail calls");

        let extended_const = adapter(
            "(global $computed i32 (i32.add (i32.const 1) (i32.const 2)))",
            &format!("{WRITE_VALID_OUTPUT} i32.const 38"),
        );
        assert!(matches!(compile(&extended_const), Err(AdapterError::Invalid(_))), "extended const");

        let multiple_memories = adapter(
            "(memory $second 1 2)",
            &format!("{WRITE_VALID_OUTPUT} (drop (memory.size $second)) i32.const 38"),
        );
        assert!(matches!(compile(&multiple_memories), Err(AdapterError::Invalid(_))), "multiple memories");

        // Atomics need a shared memory, and shared memory is not what a deterministic adapter may have:
        // whether it is refused for the atomics or for the sharing, it is refused.
        let shared = r#"(module (memory (export "memory") 1 2 shared)
            (func (export "adapter_abi_version") (result i32) i32.const 1)
            (func (export "input_buffer") (result i32) i32.const 1024)
            (func (export "input_capacity") (result i32) i32.const 4200)
            (func (export "output_buffer") (result i32) i32.const 8192)
            (func (export "adapter_run") (param i32) (result i32)
              (drop (i32.atomic.load (i32.const 0))) i32.const 38))"#;
        assert!(matches!(compile(shared), Err(AdapterError::Invalid(_))), "threads/atomics");

        // And the one that *is* allowed still is, so this test cannot pass by refusing everything.
        let bulk = adapter("", &format!("{WRITE_VALID_OUTPUT} (memory.fill (i32.const 0) (i32.const 0) (i32.const 8)) i32.const 38"));
        assert!(compile(&bulk).is_ok(), "bulk memory is the single admitted extension");
    }

    // --- bulk memory: what it costs, and what it cannot reach -----------------------------------

    /// A module with a full 16 pages of memory, so a bulk operation has a megabyte to work in, and with
    /// its output buffer far below the region it touches.
    fn bulk_module(body: &str) -> String {
        adapter("", &format!("{WRITE_VALID_OUTPUT} {body} i32.const 38"))
            .replace(r#"(memory (export "memory") 1 2)"#, r#"(memory (export "memory") 16 16)"#)
    }

    /// The length comes from the payload, not from a literal: two runs of the same module then differ in
    /// exactly one thing — how many bytes were moved — so a difference in fuel can only be that.
    const LENGTH_FROM_PAYLOAD: &str = "(i32.load (i32.const 1061))";

    fn fuel_for_bulk(module: &AdapterModule, length: u32) -> u64 {
        module.run_once(&PLAYER, &length.to_le_bytes()).unwrap().fuel_used
    }

    #[test]
    fn bulk_operations_are_metered_per_byte_and_not_per_instruction() {
        let fill = compile(&bulk_module(&format!("(memory.fill (i32.const 100000) (i32.const 7) {LENGTH_FROM_PAYLOAD})"))).unwrap();
        let copy = compile(&bulk_module(&format!("(memory.copy (i32.const 100000) (i32.const 0) {LENGTH_FROM_PAYLOAD})"))).unwrap();

        for (name, module) in [("memory.fill", &fill), ("memory.copy", &copy)] {
            let base = fuel_for_bulk(module, 0);
            // The same two instructions cost sixteen times more for sixteen times the bytes. If a bulk
            // operation were charged like an ordinary instruction, these three numbers would be equal.
            let small = fuel_for_bulk(module, 4_096) - base;
            let large = fuel_for_bulk(module, 65_536) - base;
            assert_eq!(large, 16 * small, "{name}: sixteen times the bytes must cost sixteen times the fuel");
            assert_eq!(fuel_for_bulk(module, 32_768) - base, 8 * small, "{name}: eight times the bytes, eight times the fuel");
            // Measured: wasmi charges one fuel per 64 bytes, rounded down. Pinned as a floor rather than
            // as the constant, so a wasmi update that re-prices this does not silently under-charge.
            assert!(large >= 1_024, "{name}: 64 KiB must cost at least one fuel per 64 bytes, got {large}");
            assert!(fuel_for_bulk(module, 64) - base <= 1, "{name}: a single 64-byte block is at most one fuel");
            // Fuel cannot be negative or free: an empty operation still pays for the instructions.
            assert!(base > 0, "{name}: the instructions themselves cost something");
        }
        // Fill and copy are priced the same because the charge is about bytes moved, not which op moved them.
        assert_eq!(fuel_for_bulk(&fill, 65_536), fuel_for_bulk(&copy, 65_536));
    }

    #[test]
    fn a_bulk_operation_without_the_fuel_to_pay_for_it_traps_before_it_finishes() {
        let wat = bulk_module(&format!("(memory.fill (i32.const 100000) (i32.const 7) {LENGTH_FROM_PAYLOAD})"));
        // Filling 64 KiB needs about 1,024 fuel for the bytes alone. Given 200, the module must trap: a
        // bulk operation is not a way to move a megabyte for the price of one instruction.
        let starved = RuntimeLimits { fuel_per_run: 200, ..RuntimeLimits::default() };
        let module = compile_with(&wat, starved).unwrap();
        assert_eq!(module.run_once(&PLAYER, &65_536u32.to_le_bytes()).unwrap_err(), AdapterError::OutOfFuel);
        assert_eq!(
            module.validate_result(&PLAYER, &65_536u32.to_le_bytes()).unwrap_err(),
            AdapterError::OutOfFuel,
            "the refusal must not be reachable only through run_once"
        );
        // And the same module under a budget that covers the work completes, so the trap above is the
        // metering rather than the size of the operation.
        let funded = RuntimeLimits { fuel_per_run: 2_000, ..RuntimeLimits::default() };
        let module = compile_with(&wat, funded).unwrap();
        assert_eq!(module.validate_result(&PLAYER, &65_536u32.to_le_bytes()).unwrap().output.outcome, Outcome::Win);
    }

    #[test]
    fn out_of_bounds_bulk_operations_trap_instead_of_reaching_outside_memory() {
        // `bulk_module` declares 16 pages but starts with 16, i.e. exactly 1,048,576 bytes.
        for (name, body) in [
            ("fill past the end", "(memory.fill (i32.const 1048570) (i32.const 7) (i32.const 100))"),
            ("fill the whole 32-bit range", "(memory.fill (i32.const 0) (i32.const 7) (i32.const -1))"),
            ("copy with the source past the end", "(memory.copy (i32.const 0) (i32.const 1048570) (i32.const 100))"),
            ("copy with the destination past the end", "(memory.copy (i32.const 1048570) (i32.const 0) (i32.const 100))"),
            ("copy the whole 32-bit range", "(memory.copy (i32.const 0) (i32.const 0) (i32.const -1))"),
            ("fill starting exactly at the end", "(memory.fill (i32.const 1048576) (i32.const 7) (i32.const 1))"),
        ] {
            let module = compile(&bulk_module(body)).unwrap();
            assert!(matches!(module.run_once(&PLAYER, &[]), Err(AdapterError::Trap(_))), "{name}");
            assert!(matches!(module.validate_result(&PLAYER, &[]), Err(AdapterError::Trap(_))), "{name}, through validate_result");
        }
        // Exactly filling the last byte is in bounds and must not trap, so the test above is not simply
        // proving that every bulk operation fails.
        let module = compile(&bulk_module("(memory.fill (i32.const 1048575) (i32.const 7) (i32.const 1))")).unwrap();
        assert_eq!(module.validate_result(&PLAYER, &[]).unwrap().output.outcome, Outcome::Win);
    }

    #[test]
    fn bulk_operations_are_deterministic_across_fresh_instances() {
        let module = compile(&bulk_module(&format!("(memory.copy (i32.const 100000) (i32.const 0) {LENGTH_FROM_PAYLOAD})"))).unwrap();
        let first = module.run_once(&PLAYER, &65_536u32.to_le_bytes()).unwrap();
        // Same output bytes and the same fuel, run after run: the sorted-byte heap a bulk copy relies on
        // must not make two runs differ, because fuel is part of what validate_result compares.
        for _ in 0..4 {
            assert_eq!(module.run_once(&PLAYER, &65_536u32.to_le_bytes()).unwrap(), first);
        }
        assert!(module.validate_result(&PLAYER, &65_536u32.to_le_bytes()).is_ok());
    }

    #[test]
    fn garbage_truncated_and_oversized_modules_are_refused() {
        assert!(matches!(AdapterModule::compile(b"not wasm at all", RuntimeLimits::default()), Err(AdapterError::Invalid(_))));
        assert!(matches!(AdapterModule::compile(&[], RuntimeLimits::default()), Err(AdapterError::Invalid(_))));
        let mut bytes = ::wat::parse_str(GOOD).unwrap();
        bytes.truncate(bytes.len() / 2);
        assert!(matches!(AdapterModule::compile(&bytes, RuntimeLimits::default()), Err(AdapterError::Invalid(_))));
        let small = RuntimeLimits { max_module_bytes: 100, ..RuntimeLimits::default() };
        assert!(matches!(compile_with(GOOD, small), Err(AdapterError::ModuleTooLarge { limit: 100 })));
    }

    #[test]
    fn a_module_must_export_the_whole_interface_with_the_right_types() {
        // Missing run, missing memory, and each export with a wrong signature.
        let no_run = r#"(module (memory (export "memory") 1 2)
            (func (export "adapter_abi_version") (result i32) i32.const 1)
            (func (export "input_buffer") (result i32) i32.const 1024)
            (func (export "input_capacity") (result i32) i32.const 4200)
            (func (export "output_buffer") (result i32) i32.const 8192))"#;
        assert!(matches!(compile(no_run), Err(AdapterError::MissingExport("adapter_run"))));
        let no_memory = r#"(module (memory 1 2)
            (func (export "adapter_abi_version") (result i32) i32.const 1)
            (func (export "input_buffer") (result i32) i32.const 1024)
            (func (export "input_capacity") (result i32) i32.const 4200)
            (func (export "output_buffer") (result i32) i32.const 8192)
            (func (export "adapter_run") (param i32) (result i32) i32.const 0))"#;
        assert!(matches!(compile(no_memory), Err(AdapterError::MissingExport("memory"))));
        let run_without_length = GOOD.replace(r#"(func (export "adapter_run") (param $len i32) (result i32)"#, r#"(func (export "adapter_run") (result i32)"#);
        // That edit leaves $len unused in the body only if the body never names it; GOOD never does.
        assert!(matches!(compile(&run_without_length), Err(AdapterError::BadExportType("adapter_run"))));
        let abi_as_i64 = GOOD.replace(r#"(func (export "adapter_abi_version") (result i32) i32.const 1)"#, r#"(func (export "adapter_abi_version") (result i64) i64.const 1)"#);
        assert!(matches!(compile(&abi_as_i64), Err(AdapterError::BadExportType("adapter_abi_version"))));
        let memory_as_global = GOOD.replace(r#"(memory (export "memory") 1 2)"#, r#"(memory 1 2) (global (export "memory") i32 (i32.const 0))"#);
        assert!(matches!(compile(&memory_as_global), Err(AdapterError::BadExportType("memory"))));
    }

    // --- memory ----------------------------------------------------------------------------------

    #[test]
    fn a_module_must_declare_a_memory_maximum_within_the_hosts_limit() {
        let unbounded = GOOD.replace(r#"(memory (export "memory") 1 2)"#, r#"(memory (export "memory") 1)"#);
        assert!(matches!(compile(&unbounded), Err(AdapterError::MemoryDeclaration(_))), "no maximum");
        let big = GOOD.replace(r#"(memory (export "memory") 1 2)"#, r#"(memory (export "memory") 1 17)"#);
        assert!(matches!(compile(&big), Err(AdapterError::MemoryDeclaration(_))), "17 pages against a limit of 16");
        let at_the_limit = GOOD.replace(r#"(memory (export "memory") 1 2)"#, r#"(memory (export "memory") 1 16)"#);
        assert!(compile(&at_the_limit).is_ok(), "exactly the limit is allowed");
        let tight = RuntimeLimits { max_memory_pages: 1, ..RuntimeLimits::default() };
        assert!(matches!(compile_with(GOOD, tight), Err(AdapterError::MemoryDeclaration(_))), "the same module under a tighter host");
    }

    #[test]
    fn growing_memory_past_the_limit_fails_the_same_way_on_every_run() {
        // The module tries to grow by `pages`, and reports what memory.grow returned as its score.
        let grower = |pages: u32| {
            adapter(
                "",
                &format!(
                    "{WRITE_VALID_OUTPUT} (i32.store (i32.const 8194) (memory.grow (i32.const {pages}))) i32.const 38"
                ),
            )
            .replace(r#"(memory (export "memory") 1 2)"#, r#"(memory (export "memory") 1 4)"#)
        };
        let limits = RuntimeLimits { max_memory_pages: 4, ..RuntimeLimits::default() };
        let within = compile_with(&grower(3), limits.clone()).unwrap();
        assert_eq!(within.validate_result(&PLAYER, &[]).unwrap().output.score, 1, "growing from 1 to 4 pages succeeded: the old size");
        let beyond = compile_with(&grower(4), limits).unwrap();
        let result = beyond.validate_result(&PLAYER, &[]).unwrap();
        assert_eq!(result.output.score, u32::MAX, "growing past the limit returned -1, identically in both runs");
    }

    // --- resource exhaustion and traps ----------------------------------------------------------

    #[test]
    fn an_endless_loop_runs_out_of_fuel_instead_of_hanging() {
        let spin = adapter("", "(loop $l (br $l)) i32.const 38");
        let module = compile(&spin).unwrap();
        assert_eq!(module.run_once(&PLAYER, &[]).unwrap_err(), AdapterError::OutOfFuel);
        // Fuel is a budget the host sets, and a legitimate adapter that needs more than it is refused too.
        let starved = RuntimeLimits { fuel_per_run: 200, ..RuntimeLimits::default() };
        let module = compile_with(GOOD, starved).unwrap();
        assert_eq!(module.run_once(&PLAYER, &[1; 1000]).unwrap_err(), AdapterError::OutOfFuel);
    }

    #[test]
    fn a_start_function_runs_under_the_same_fuel() {
        let wat = adapter("(func $boot (loop $l (br $l))) (start $boot)", &format!("{WRITE_VALID_OUTPUT} i32.const 38"));
        let module = compile(&wat).unwrap();
        assert_eq!(module.run_once(&PLAYER, &[]).unwrap_err(), AdapterError::OutOfFuel);
    }

    #[test]
    fn traps_are_reported_and_never_become_a_result() {
        for (name, body) in [
            ("unreachable", "unreachable i32.const 38"),
            ("division by zero", "(drop (i32.div_u (i32.const 1) (i32.const 0))) i32.const 38"),
            ("out of bounds read", "(drop (i32.load (i32.const 70000))) i32.const 38"),
            ("out of bounds write", "(i32.store (i32.const 70000) (i32.const 1)) i32.const 38"),
        ] {
            let module = compile(&adapter("", body)).unwrap();
            assert!(matches!(module.run_once(&PLAYER, &[]), Err(AdapterError::Trap(_))), "{name}");
            assert!(matches!(module.validate_result(&PLAYER, &[]), Err(AdapterError::Trap(_))), "{name}, through validate_result");
        }
    }

    #[test]
    fn unbounded_recursion_traps_on_depth_or_fuel_and_never_overflows_the_host() {
        let wat = adapter("(func $down (call $down))", "(call $down) i32.const 38");
        let module = compile(&wat).unwrap();
        assert!(matches!(
            module.run_once(&PLAYER, &[]),
            Err(AdapterError::Trap(_)) | Err(AdapterError::OutOfFuel)
        ));
    }

    // --- the interface contract -----------------------------------------------------------------

    #[test]
    fn another_abi_version_is_refused_and_so_is_a_module_that_lies_about_its_output() {
        let wrong_abi = GOOD.replace(r#"(func (export "adapter_abi_version") (result i32) i32.const 1)"#, r#"(func (export "adapter_abi_version") (result i32) i32.const 2)"#);
        assert_eq!(compile(&wrong_abi).unwrap().run_once(&PLAYER, &[]).unwrap_err(), AdapterError::AbiVersion { found: 2 });
        // A refusal code is the module's own business and is passed up as such.
        let refuses = adapter("", "i32.const -7");
        assert_eq!(compile(&refuses).unwrap().run_once(&PLAYER, &[]).unwrap_err(), AdapterError::GuestRefused(-7));
        // Wrong length, wrong version byte, an outcome that does not exist, an output outside memory.
        let wrong_length = adapter("", &format!("{WRITE_VALID_OUTPUT} i32.const 100"));
        assert!(matches!(compile(&wrong_length).unwrap().run_once(&PLAYER, &[]), Err(AdapterError::BadOutput(_))));
        let wrong_version_byte = adapter("", "(i32.store8 (i32.const 8192) (i32.const 9)) i32.const 38");
        assert!(matches!(compile(&wrong_version_byte).unwrap().run_once(&PLAYER, &[]), Err(AdapterError::BadOutput(_))));
        let unknown_outcome = adapter("", "(i32.store8 (i32.const 8192) (i32.const 1)) (i32.store8 (i32.const 8193) (i32.const 9)) i32.const 38");
        assert!(matches!(compile(&unknown_outcome).unwrap().run_once(&PLAYER, &[]), Err(AdapterError::BadOutput(_))));
        let outside = GOOD.replace(r#"(func (export "output_buffer") (result i32) i32.const 8192)"#, r#"(func (export "output_buffer") (result i32) i32.const 65530)"#);
        assert!(matches!(compile(&outside).unwrap().run_once(&PLAYER, &[1]), Err(AdapterError::BadOutput(_))), "the output region runs off the end of memory");
        let negative = GOOD.replace(r#"(func (export "input_buffer") (result i32) i32.const 1024)"#, r#"(func (export "input_buffer") (result i32) i32.const -1)"#);
        assert!(matches!(compile(&negative).unwrap().run_once(&PLAYER, &[1]), Err(AdapterError::BadOutput(_))), "a negative address");
    }

    #[test]
    fn the_output_has_no_way_to_claim_it_is_verified() {
        // The whole output is fixed and parsed by the host: an outcome code, a score and a digest. A module
        // that writes any other byte for the outcome is refused, so there is no flag to set.
        let module = compile(GOOD).unwrap();
        let record = module.run_once(&PLAYER, &[1]).unwrap();
        assert_eq!(record.raw_output.len(), OUTPUT_BYTES);
        assert_eq!(parse_output(&record.raw_output).unwrap(), record.output);
        for code in 5..=255u8 {
            let mut raw = record.raw_output.clone();
            raw[1] = code;
            assert!(matches!(parse_output(&raw), Err(AdapterError::BadOutput(_))), "outcome {code}");
        }
    }

    #[test]
    fn input_is_bounded_by_the_host_and_by_the_modules_own_capacity() {
        let module = compile(GOOD).unwrap();
        let too_big = vec![1u8; RuntimeLimits::default().max_payload_bytes + 1];
        assert_eq!(module.run_once(&PLAYER, &too_big).unwrap_err(), AdapterError::InputTooLarge);
        assert!(module.run_once(&PLAYER, &vec![1u8; RuntimeLimits::default().max_payload_bytes]).is_ok(), "exactly the limit is accepted");
        let small_capacity = GOOD.replace(r#"(func (export "input_capacity") (result i32) i32.const 4200)"#, r#"(func (export "input_capacity") (result i32) i32.const 40)"#);
        let module = compile(&small_capacity).unwrap();
        assert!(module.run_once(&PLAYER, &[1, 2]).is_ok());
        assert_eq!(module.run_once(&PLAYER, &[1; 10]).unwrap_err(), AdapterError::InputTooLarge, "the module said it holds 40 bytes");
    }

    #[test]
    fn validation_requires_two_agreeing_runs_and_refuses_otherwise() {
        let module = compile(GOOD).unwrap();
        let record = module.run_once(&PLAYER, &[1]).unwrap();
        // Agreeing runs are accepted and carry the module's identity.
        let accepted = validate_with([5; 32], || Ok(record.clone())).unwrap();
        assert_eq!((accepted.output, accepted.module_hash), (record.output.clone(), [5; 32]));
        // The second run differs in its output: refused.
        let mut calls = 0;
        let disagreeing_output = validate_with([5; 32], || {
            calls += 1;
            let mut next = record.clone();
            if calls == 2 {
                next.raw_output[2] ^= 1;
            }
            Ok(next)
        });
        assert_eq!(disagreeing_output.err(), Some(AdapterError::Nondeterministic));
        // The second run differs only in the work it took: refused.
        let mut calls = 0;
        let disagreeing_work = validate_with([5; 32], || {
            calls += 1;
            let mut next = record.clone();
            next.fuel_used += calls;
            Ok(next)
        });
        assert_eq!(disagreeing_work.err(), Some(AdapterError::Nondeterministic));
        // A second run that fails is a failure, not an agreement.
        let mut calls = 0;
        let second_traps = validate_with([5; 32], || {
            calls += 1;
            if calls == 2 { Err(AdapterError::OutOfFuel) } else { Ok(record.clone()) }
        });
        assert_eq!(second_traps.err(), Some(AdapterError::OutOfFuel));
    }

    #[test]
    fn a_module_is_identified_by_its_exact_bytes() {
        let a = compile(GOOD).unwrap();
        let other = GOOD.replace("i32.const 4200", "i32.const 4100"); // still a valid module, one constant different
        let b = compile(&other).unwrap();
        assert_ne!(a.hash(), b.hash(), "a different byte is a different module");
        let bytes = ::wat::parse_str(GOOD).unwrap();
        assert_eq!(a.hash(), <[u8; 32]>::from(Sha3_256::digest(&bytes)), "the hash is SHA3-256 of the exact bytes");
        assert_eq!(a.hash(), compile(GOOD).unwrap().hash(), "and it is stable");
        // A trailing byte makes it a different file, whatever it does.
        let mut changed = bytes.clone();
        changed.push(0);
        assert_ne!(<[u8; 32]>::from(Sha3_256::digest(&changed)), a.hash());
    }
}
