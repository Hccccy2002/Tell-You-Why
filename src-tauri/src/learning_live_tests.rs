//! Opt-in acceptance against an existing configured provider. Results use a scratch database.
use crate::{
    db::Database, providers::RestrictedHttpClient, secret_store::WindowsCredentialStore, AppState,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

#[test]
#[ignore = "sends up to ten explicitly selected textbook packets to an existing verified provider"]
fn learning_live_random_acceptance() {
    tauri::async_runtime::block_on(async {
        let snapshot = PathBuf::from(
            std::env::var_os("TELLWHY_LEARNING_LIVE_PROFILE_DB")
                .expect("explicit profile snapshot required"),
        );
        let provider =
            std::env::var("TELLWHY_LEARNING_LIVE_PROVIDER").expect("explicit provider required");
        let profile = Database::new(snapshot)
            .provider_profiles()
            .unwrap()
            .into_iter()
            .find(|p| p.provider_id == provider && p.connection_verified && p.key_last4.is_some())
            .expect("verified provider required");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tmp/learning-reports");
        let packets: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join("packets.json")).unwrap()).unwrap();
        assert!(packets.len() <= 10);
        let db_path = root.join(format!("live-{}.db", uuid::Uuid::new_v4()));
        let database = Database::new(db_path.clone());
        database.initialize().unwrap();
        database.save_provider_profile(&profile).unwrap();
        let state = AppState {
            database,
            secrets: Arc::new(WindowsCredentialStore),
            http: Arc::new(RestrictedHttpClient::new().unwrap()),
            mcp: Arc::new(crate::mcp::McpRuntime::new().unwrap()),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };
        let mut outcomes = vec![];
        for packet in packets {
            let task = json!({"id":uuid::Uuid::new_v4().to_string(),"kb":packet["kb"],"kind":"card","state":"prepared","created_at":chrono::Utc::now().to_rfc3339(),"provider":profile.provider_id,"region":profile.region,"model":profile.model,"packet":packet,"result":null,"usage":[],"prompt_version":crate::rag::PROMPT_VERSION,"error":null});
            state.database.rag_insert(&task).unwrap();
            let result =
                crate::rag_commands::generate_inner(task["id"].as_str().unwrap().into(), &state)
                    .await;
            let result = match result {
                Ok(task) => task,
                Err(error) => json!({"state":"error","error":error}),
            };
            let transport_failed = result["state"] == "error"
                || result["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("教材模型请求失败"));
            if let Some(card_id) = result["card_id"].as_str() {
                let learning = state.database.learning_state(card_id).unwrap();
                assert_eq!(learning.status, "new");
                assert_eq!(learning.shown_count, 0);
            }
            println!(
                "sample {}: state={}, result={}",
                outcomes.len() + 1,
                result["state"],
                result["result"]["status"]
            );
            outcomes.push(result);
            std::fs::write(root.join("live-acceptance.json"),serde_json::to_vec_pretty(&json!({"database":db_path,"provider":profile.provider_id,"model":profile.model,"results":outcomes})).unwrap()).unwrap();
            if transport_failed {
                break;
            }
        }
        assert_eq!(
            outcomes.len(),
            10,
            "transport/configuration failure stopped live acceptance"
        );
        assert!(
            outcomes
                .iter()
                .all(|task| task["state"] == "completed" && task["result"]["status"] == "answered"),
            "inspect the report: some material was withheld or could not support a card"
        );
        let reopened = Database::new(db_path);
        reopened.initialize().unwrap();
        assert!(
            reopened.rag_list("computer-organization", true, 0).unwrap()["total"]
                .as_u64()
                .unwrap()
                > 0
        );
    });
}
