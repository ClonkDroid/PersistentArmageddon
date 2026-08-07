//! Minimal C-ABI boundary usable from WebAssembly without owning simulation truth.
use sim_core::World;

pub extern "C" fn deterministic_empty_world_digest(seed: u64) -> u64 {
    World::new(seed).state_digest()
}
