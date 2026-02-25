use kash_server::constants::*;
use kash_server::models::*;
#[test]
fn serde_send_friend_request_payload() {
    let json = r#"{"friend_username":"alice"}"#;
    let payload: SendFriendRequestPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.friend_username, "alice");
    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains("alice"));
}

#[test]
fn serde_accept_friend_payload() {
    let json = r#"{"friend_id":"user-123"}"#;
    let payload: AcceptFriendPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.friend_id, "user-123");
    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains("user-123"));
}

#[test]
fn serde_update_nickname_payload() {
    let json = r#"{"friend_id":"user-456","nickname":"Bob Smith"}"#;
    let payload: UpdateNicknamePayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.friend_id, "user-456");
    assert_eq!(payload.nickname, Some("Bob Smith".to_string()));
}

#[test]
fn serde_update_nickname_payload_none() {
    let json = r#"{"friend_id":"user-456","nickname":null}"#;
    let payload: UpdateNicknamePayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.friend_id, "user-456");
    assert_eq!(payload.nickname, None);
}

#[test]
fn serde_remove_friend_payload() {
    let json = r#"{"friend_id":"user-999"}"#;
    let payload: RemoveFriendPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.friend_id, "user-999");
}

#[test]
fn serde_friendship_relation_roundtrip() {
    let relation = FriendshipRelation {
        id: "rel-001".to_string(),
        user_id: "user-123".to_string(),
        pending: false,
        nickname: Some("Best Friend".to_string()),
    };
    let json = serde_json::to_string(&relation).unwrap();
    let deserialized: FriendshipRelation = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "rel-001");
    assert_eq!(deserialized.user_id, "user-123");
    assert!(!deserialized.pending);
    assert_eq!(deserialized.nickname, Some("Best Friend".to_string()));
}

#[test]
fn serde_friendship_relation_no_nickname() {
    let json = r#"{"id":"rel-002","user_id":"user-456","pending":true,"nickname":null}"#;
    let relation: FriendshipRelation = serde_json::from_str(json).unwrap();
    assert_eq!(relation.id, "rel-002");
    assert!(relation.pending);
    assert_eq!(relation.nickname, None);
}

#[test]
fn serde_split_participant() {
    let json = r#"{"user_id":"user-123","amount":50.0}"#;
    let participant: SplitParticipant = serde_json::from_str(json).unwrap();
    assert_eq!(participant.user_id, "user-123");
    assert_eq!(participant.amount, 50.0);
}

#[test]
fn serde_create_split_payload() {
    let json = r#"{
        "idempotency_key":"idempotency-123",
        "total_amount":120.0,
        "description":"Dinner with friends",
        "date":"2025-02-16",
        "category_id":"cat-dining",
        "splits":[
            {"user_id":"user-1","amount":40.0},
            {"user_id":"user-2","amount":40.0},
            {"user_id":"user-3","amount":40.0}
        ]
    }"#;
    let payload: CreateSplitPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.idempotency_key, "idempotency-123");
    assert_eq!(payload.total_amount, 120.0);
    assert_eq!(payload.description, "Dinner with friends");
    assert_eq!(payload.date, "2025-02-16");
    assert_eq!(payload.category_id, "cat-dining");
    assert_eq!(payload.splits.len(), 3);
    assert_eq!(payload.splits[0].amount, 40.0);
}

#[test]
fn serde_finalize_pending_payload() {
    let json = r#"{"record_id":"rec-001","category_id":"cat-misc"}"#;
    let payload: FinalizePendingPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.record_id, "rec-001");
    assert_eq!(payload.category_id, "cat-misc");
}

#[test]
fn serde_update_settle_payload() {
    let json = r#"{"split_id":"split-001"}"#;
    let payload: UpdateSettlePayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.split_id, "split-001");
}

#[test]
fn serde_split_record_roundtrip() {
    let record = SplitRecord {
        id: "split-001".to_string(),
        payer_id: "user-123".to_string(),
        total_amount: 150.0,
        description: "Lunch split".to_string(),
        date: "2025-02-16".to_string(),
        status: SPLIT_STATUS_INITIATED.to_string(),
        created_at: "2025-02-16T10:00:00".to_string(),
        updated_at: "2025-02-16T10:00:00".to_string(),
    };
    let json = serde_json::to_string(&record).unwrap();
    let deserialized: SplitRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "split-001");
    assert_eq!(deserialized.payer_id, "user-123");
    assert_eq!(deserialized.total_amount, 150.0);
    assert_eq!(deserialized.status, SPLIT_STATUS_INITIATED);
}
