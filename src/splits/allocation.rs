use crate::money::to_cents;

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

        if !split.amount.is_finite() {
            return Err("Amount must be a valid finite number".to_string());
        }

        if split.amount <= 0.0 || to_cents(split.amount) == 0 {
            return Err("Amount must be positive".to_string());
        }
    }

    Ok(())
}

pub fn calculate_split_amounts(
    total: f64,
    splits: Vec<crate::models::SplitParticipant>,
    initiator_id: &str,
) -> Result<Vec<(String, i64)>, String> {
    if !total.is_finite() || total <= 0.0 || to_cents(total) == 0 {
        return Err("Total amount must be a positive finite number".to_string());
    }

    let total_cents = to_cents(total);
    let mut participant_amounts = Vec::with_capacity(splits.len());
    for split in splits {
        if !split.amount.is_finite() {
            return Err("Amount must be a valid finite number".to_string());
        }
        if split.amount <= 0.0 || to_cents(split.amount) == 0 {
            return Err("Amount must be positive".to_string());
        }
        participant_amounts.push((split.user_id, to_cents(split.amount)));
    }
    let total_split_cents: i64 = participant_amounts.iter().map(|(_, amount)| amount).sum();
    let initiator_amount = total_cents - total_split_cents;

    if initiator_amount < 0 {
        return Err("Split sum exceeds total".to_string());
    }

    let mut result = Vec::with_capacity(participant_amounts.len() + 1);
    result.push((initiator_id.to_string(), initiator_amount));
    result.extend(participant_amounts);

    Ok(result)
}
