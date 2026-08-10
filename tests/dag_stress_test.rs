// tests/dag_stress_test.rs
// ============================================================
// DAG SCALABILITY STRESS TEST - 100 VALIDATORS
// ============================================================

use parking_lot::RwLock;
use std::fs;
use std::sync::Arc;
use std::time::Instant;
use UltraNet::*;

fn cleanup(path: &str) {
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn test_dag_scalability_100_validators() {
    let db_path = "test_db_dag_stress";
    cleanup(db_path);

    // 1. Initialize Infrastructure
    let storage = Arc::new(Storage::new(db_path).expect("Failed to open database"));
    let shared_storage = Arc::new(SharedStorage {
        storage: storage.clone(),
        dag_tree: storage
            .db
            .open_tree("dag_vertices_stress")
            .expect("Failed to open tree"),
        move_modules: storage
            .db
            .open_tree("move_modules_stress")
            .expect("Failed to open tree"),
        move_resources: storage
            .db
            .open_tree("move_resources_stress")
            .expect("Failed to open tree"),
        fhe_keys: storage
            .db
            .open_tree("fhe_keys_stress")
            .expect("Failed to open tree"),
        trie_shards: storage.trie_shards.clone(),
        reference_count: 1,
    });

    let validator_count = 100;
    let faulty_count = 33; // 3f + 1 model

    let dag = Arc::new(RwLock::new(MysticetiDAG::new(
        validator_count,
        faulty_count,
        shared_storage.clone(),
    )));
    let mut bullshark =
        BullsharkDAG::new_with_dag(validator_count, faulty_count, dag.clone(), shared_storage);

    println!(
        "🚀 Starting DAG Stress Test with {} validators...",
        validator_count
    );
    let total_start = Instant::now();

    // 2. Genesis Round
    let genesis = MysticetiVertex::new(0, 0, vec![]);
    let genesis_hash = genesis.hash;
    bullshark
        .add_vertex(genesis)
        .expect("Failed to add genesis");

    let mut last_round_hashes = vec![genesis_hash];

    // 3. Simulate 10 Rounds
    for round in 1..=10 {
        let round_start = Instant::now();
        let mut current_round_hashes = Vec::new();

        println!("📦 Processing Round {}...", round);

        for v_id in 0..validator_count {
            // Each validator references all vertices from previous round (Fan-out stress)
            let vertex = MysticetiVertex::new(v_id as u64, round, last_round_hashes.clone());
            let hash = vertex.hash;

            bullshark.add_vertex(vertex).expect("Failed to add vertex");
            current_round_hashes.push(hash);
        }

        let round_duration = round_start.elapsed();
        println!("   Round {} completed in {:?}", round, round_duration);

        // Verify Anchors
        if let Some(anchor_hash) = bullshark.get_anchor(round) {
            println!(
                "   ✅ Anchor found for round {}: {:x?}",
                round,
                &anchor_hash[..4]
            );
        } else {
            // Note: In Round 1, anchor might not trigger if has_quorum logic requires references
            // from Round 2. We check stability regardless.
            println!(
                "   ⚠️ No anchor yet for round {} (expected in async DAG)",
                round
            );
        }

        last_round_hashes = current_round_hashes;
    }

    let total_duration = total_start.elapsed();
    let stats = bullshark.get_stats();

    println!("\n📊 STRESS TEST RESULTS:");
    println!("   Total Validators: {}", validator_count);
    println!("   Total Rounds: 10");
    println!(
        "   Total Vertices Processed: {}",
        stats.get("total_vertices").unwrap_or(&"0".to_string())
    );
    println!(
        "   Total Anchors Identified: {}",
        stats.get("anchors").unwrap_or(&"0".to_string())
    );
    println!("   Total Execution Time: {:?}", total_duration);
    println!(
        "   Avg Time Per Vertex: {:?}",
        total_duration / (10 * validator_count as u32)
    );

    assert!(
        total_duration.as_secs() < 30,
        "DAG performance too slow for 100 validators!"
    );

    cleanup(db_path);
}
