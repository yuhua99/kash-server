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
        nickname: "Best Friend".to_string(),
    };
    let json = serde_json::to_string(&relation).unwrap();
    let deserialized: FriendshipRelation = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "rel-001");
    assert_eq!(deserialized.user_id, "user-123");
    assert!(!deserialized.pending);
    assert_eq!(deserialized.nickname, "Best Friend");
}

#[test]
fn serde_friendship_relation_no_nickname() {
    let json = r#"{"id":"rel-002","user_id":"user-456","pending":true,"nickname":"user-456"}"#;
    let relation: FriendshipRelation = serde_json::from_str(json).unwrap();
    assert_eq!(relation.id, "rel-002");
    assert!(relation.pending);
    assert_eq!(relation.nickname, "user-456");
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
        "total_amount": 120.0,
        "currency": "TWD",
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
fn serde_split_created_response_roundtrip() {
    let response = SplitCreatedResponse {
        split_id: "split-001".to_string(),
        creditor_record_id: "record-001".to_string(),
        participants: vec![ParticipantBrief {
            id: "participant-001".to_string(),
            debtor_user_id: "user-123".to_string(),
            amount: 50.0,
        }],
    };
    let json = serde_json::to_string(&response).unwrap();
    let deserialized: SplitCreatedResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.split_id, "split-001");
    assert_eq!(deserialized.creditor_record_id, "record-001");
    assert_eq!(deserialized.participants.len(), 1);
    assert_eq!(deserialized.participants[0].debtor_user_id, "user-123");
    assert_eq!(deserialized.participants[0].amount, 50.0);
}
