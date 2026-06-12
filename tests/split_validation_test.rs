mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::SplitParticipant;
use kash_server::splits::{calculate_split_amounts, validate_split_participants};
use serde_json::json;
use tower::util::ServiceExt;

#[test]
fn test_validate_split_participants_exact_match() {
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 300.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 200.0,
        },
    ];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(
        result.is_ok(),
        "Valid splits should pass validation: {:?}",
        result.err()
    );
}

#[test]
fn test_validate_split_participants_with_remainder() {
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 350.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 250.0,
        },
    ];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(
        result.is_ok(),
        "Valid splits with remainder should pass validation: {:?}",
        result.err()
    );
}

#[test]
fn test_validate_split_participants_duplicate_participant() {
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 300.0,
        },
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 200.0,
        },
    ];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(
        result.is_err(),
        "Duplicate participants should fail validation"
    );
    assert_eq!(result.unwrap_err(), "Duplicate participant: B");
}

#[test]
fn test_validate_split_participants_initiator_in_splits() {
    let splits = vec![
        SplitParticipant {
            user_id: "A".to_string(),
            amount: 300.0,
        },
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 200.0,
        },
    ];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(
        result.is_err(),
        "Initiator appearing in splits should fail validation"
    );
    assert_eq!(result.unwrap_err(), "Duplicate participant: A");
}

#[test]
fn test_validate_split_participants_negative_amount() {
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: -100.0,
    }];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(result.is_err(), "Negative amounts should fail validation");
    assert_eq!(result.unwrap_err(), "Amount must be positive");
}

#[test]
fn test_validate_split_participants_zero_amount() {
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 0.0,
    }];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(result.is_err(), "Zero amounts should fail validation");
    assert_eq!(result.unwrap_err(), "Amount must be positive");
}

#[test]
fn test_validate_split_participants_nan_amount() {
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: f64::NAN,
    }];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(result.is_err(), "NaN amounts should fail validation");
    assert_eq!(result.unwrap_err(), "Amount must be a valid finite number");
}

#[test]
fn test_validate_split_participants_infinity_amount() {
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: f64::INFINITY,
    }];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(result.is_err(), "Infinity amounts should fail validation");
    assert_eq!(result.unwrap_err(), "Amount must be a valid finite number");
}

#[test]
fn test_validate_split_participants_amount_exceeds_maximum() {
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 2_000_000_000.0,
    }];
    let initiator = "A";

    let result = validate_split_participants(&splits, initiator);
    assert!(result.is_err(), "Oversized amounts should fail validation");
    assert_eq!(result.unwrap_err(), "Amount exceeds maximum allowed value");
}

#[tokio::test]
async fn split_create_participant_amount_exceeds_maximum_returns_bad_request() {
    let app = common::setup_test_app().await.expect("setup failed");
    let _alice_id = common::create_test_user(&app.state, "alice_split_max", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_split_max", "pw")
        .await
        .expect("create bob");
    let cookie = common::login_user(&app.router, "alice_split_max", "pw")
        .await
        .expect("login alice");

    let request = Request::builder()
        .method("POST")
        .uri("/splits/create")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "idempotency_key": "split-max-participant",
                "total_amount": 100.0,
                "currency": "TWD",
                "description": "oversized split",
                "date": "2026-02-20",
                "category_id": "cat-placeholder",
                "splits": [{ "user_id": bob_id, "amount": 2_000_000_000.0 }]
            })
            .to_string(),
        ))
        .expect("build split request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_calculate_split_amounts_exact_match() {
    let total = 1000.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 300.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 200.0,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Exact match should succeed");

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 3, "Should have 3 participants");

    // Find each participant's amount
    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(
        map.get("A").copied(),
        Some(50000),
        "Initiator should get 500.0"
    );
    assert_eq!(map.get("B").copied(), Some(30000), "B should get 300.0");
    assert_eq!(map.get("C").copied(), Some(20000), "C should get 200.0");
}

#[test]
fn test_calculate_split_amounts_with_remainder() {
    let total = 1000.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 350.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 250.0,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Remainder case should succeed");

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 3, "Should have 3 participants");

    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(
        map.get("A").copied(),
        Some(40000),
        "Initiator should get 400.0"
    );
    assert_eq!(map.get("B").copied(), Some(35000), "B should get 350.0");
    assert_eq!(map.get("C").copied(), Some(25000), "C should get 250.0");

    // Verify sum equals total
    let sum: i64 = map.values().sum();
    assert_eq!(sum, 100000, "Sum should equal total cents");
}

#[test]
fn test_calculate_split_amounts_exceeds_total() {
    let total = 1000.0;
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 1200.0,
    }];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_err(), "Sum exceeding total should fail");
    assert_eq!(result.unwrap_err(), "Split sum exceeds total");
}

#[test]
fn test_calculate_split_amounts_sum_equals_total() {
    let total = 1000.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 600.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 400.0,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Sum equal to total should succeed");

    let amounts = result.unwrap();
    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(map.get("A").copied(), Some(0), "Initiator should get 0.0");
    assert_eq!(map.get("B").copied(), Some(60000), "B should get 600.0");
    assert_eq!(map.get("C").copied(), Some(40000), "C should get 400.0");
}

#[test]
fn test_calculate_split_amounts_floating_point_rounding() {
    let total = 100.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 33.33,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 33.33,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Floating point amounts should succeed");

    let amounts = result.unwrap();
    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    let b_amount = map.get("B").copied().unwrap();
    let c_amount = map.get("C").copied().unwrap();
    let a_amount = map.get("A").copied().unwrap();

    // Verify amounts are rounded to 2 decimals
    assert_eq!(b_amount, 3333, "B amount should be 3333 cents");
    assert_eq!(c_amount, 3333, "C amount should be 3333 cents");
    assert_eq!(a_amount, 3334, "A (initiator) should get the remainder");

    // Verify sum equals total
    let sum = a_amount + b_amount + c_amount;
    assert_eq!(sum, 10000, "Sum should equal total cents");
}

#[test]
fn test_calculate_split_amounts_three_participants_leaves_one_cent_for_initiator() {
    let total = 100.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 33.33,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 33.33,
        },
        SplitParticipant {
            user_id: "D".to_string(),
            amount: 33.33,
        },
    ];

    let amounts = calculate_split_amounts(total, splits, "A").expect("split calculation");
    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(map.get("A").copied(), Some(1));
    assert_eq!(map.values().sum::<i64>(), 10000);
}

#[test]
fn test_calculate_split_amounts_single_participant() {
    let total = 500.0;
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 200.0,
    }];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Single participant should succeed");

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 2, "Should have 2 participants");

    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(
        map.get("A").copied(),
        Some(30000),
        "Initiator should get 300.0"
    );
    assert_eq!(map.get("B").copied(), Some(20000), "B should get 200.0");
}

#[test]
fn test_calculate_split_amounts_multiple_participants() {
    let total = 1500.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 400.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 350.0,
        },
        SplitParticipant {
            user_id: "D".to_string(),
            amount: 300.0,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok(), "Multiple participants should succeed");

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 4, "Should have 4 participants");

    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    assert_eq!(
        map.get("A").copied(),
        Some(45000),
        "Initiator should get 450.0"
    );
    assert_eq!(map.get("B").copied(), Some(40000), "B should get 400.0");
    assert_eq!(map.get("C").copied(), Some(35000), "C should get 350.0");
    assert_eq!(map.get("D").copied(), Some(30000), "D should get 300.0");

    let sum: i64 = map.values().sum();
    assert_eq!(sum, 150000, "Sum should equal total cents");
}

#[test]
fn test_calculate_split_amounts_invalid_total() {
    let total = -1000.0;
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 300.0,
    }];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_err(), "Negative total should fail");
}

#[test]
fn test_calculate_split_amounts_zero_total() {
    let total = 0.0;
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 300.0,
    }];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_err(), "Zero total should fail");
}

#[test]
fn test_calculate_split_amounts_nan_total() {
    let total = f64::NAN;
    let splits = vec![SplitParticipant {
        user_id: "B".to_string(),
        amount: 300.0,
    }];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_err(), "NaN total should fail");
}

#[test]
fn test_remainder_to_initiator_happy_path() {
    // This test verifies the core requirement: remainder goes to initiator
    let total = 1000.0;
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 350.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 250.0,
        },
    ];
    let initiator = "A";

    let result = calculate_split_amounts(total, splits, initiator);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    let mut map = std::collections::HashMap::new();
    for (user_id, amount) in amounts {
        map.insert(user_id, amount);
    }

    // 350 + 250 = 600, remainder = 1000 - 600 = 400 goes to initiator
    assert_eq!(
        map.get("A").copied(),
        Some(40000),
        "Remainder must go to initiator"
    );
}

#[test]
fn test_split_validation_and_calculation_integration() {
    let splits = vec![
        SplitParticipant {
            user_id: "B".to_string(),
            amount: 300.0,
        },
        SplitParticipant {
            user_id: "C".to_string(),
            amount: 200.0,
        },
    ];
    let initiator = "A";
    let total = 1000.0;

    // First validate
    let validation_result = validate_split_participants(&splits, initiator);
    assert!(validation_result.is_ok());

    // Then calculate
    let calc_result = calculate_split_amounts(total, splits, initiator);
    assert!(calc_result.is_ok());

    let amounts = calc_result.unwrap();
    let sum: i64 = amounts.iter().map(|(_, amt)| amt).sum();
    assert_eq!(sum, 100000, "All amounts must sum to total cents");
}
