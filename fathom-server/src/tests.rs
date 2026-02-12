use crate::runtime::Runtime;

#[tokio::test]
async fn creates_session_with_profile_copies() {
    let runtime = Runtime::new(2, 10);
    let session = runtime
        .create_session("agent-a".to_string(), vec!["user-a".to_string()])
        .await
        .expect("create session");

    assert_eq!(session.agent_id, "agent-a");
    assert_eq!(session.participant_user_ids, vec!["user-a".to_string()]);
    assert!(session.agent_profile_copy.is_some());
    assert_eq!(session.participant_user_profiles_copy.len(), 1);
}
