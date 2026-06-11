/// Validates split participants for consistency and validity.
///
/// Checks:
/// - No duplicate user_ids (including initiator appearing in splits)
/// - All amounts are strictly positive (> 0.0)
/// - Amounts are finite (no NaN or infinity)
///
/// # Errors
/// Returns descriptive error messages for validation failures.
pub fn validate_split_participants(
    splits: &[crate::models::SplitParticipant],
    initiator_id: &str,
) -> Result<(), String> {
    let mut seen_ids = std::collections::HashSet::new();
    seen_ids.insert(initiator_id.to_string());

    for split in splits {
        if !seen_ids.insert(split.user_id.clone()) {
            return Err(format!("Duplicate participant: {}", split.user_id));
        }

        if split.amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }

        if !split.amount.is_finite() {
            return Err("Amount must be a valid finite number".to_string());
        }
    }

    Ok(())
}

/// Calculates final split amounts with deterministic remainder assignment.
///
/// If the sum of split amounts is less than the total, the remainder is
/// assigned to the initiator. Returns all participants (including initiator)
/// with their final amounts, rounded to 2 decimals.
///
/// # Errors
/// Returns error if:
/// - Split sum exceeds total_amount
/// - Any amount is not finite
pub fn calculate_split_amounts(
    total: f64,
    splits: Vec<crate::models::SplitParticipant>,
    initiator_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    if !total.is_finite() || total <= 0.0 {
        return Err("Total amount must be a positive finite number".to_string());
    }

    let mut total_split = 0.0;
    for split in &splits {
        let rounded = (split.amount * 100.0).round() / 100.0;
        total_split += rounded;
    }

    total_split = (total_split * 100.0).round() / 100.0;

    if total_split > total {
        return Err("Split sum exceeds total".to_string());
    }

    let mut result = Vec::new();

    let initiator_amount = total - total_split;
    let initiator_amount = (initiator_amount * 100.0).round() / 100.0;
    result.push((initiator_id.to_string(), initiator_amount));

    for split in splits {
        let rounded = (split.amount * 100.0).round() / 100.0;
        result.push((split.user_id, rounded));
    }

    Ok(result)
}
