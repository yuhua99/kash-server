const USERNAME_PATTERN = /^[A-Za-z0-9_-]+$/;
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

export function validateUsername(value: string): string | null {
  if (value.length < 4 || value.length > 50) {
    return "Username must be 4-50 characters.";
  }

  if (!USERNAME_PATTERN.test(value)) {
    return "Username allows letters, numbers, _ and - only.";
  }

  return null;
}

export function validatePassword(value: string): string | null {
  if (value.length < 6) {
    return "Password must be at least 6 characters.";
  }

  return null;
}

export function validateCategoryName(value: string): string | null {
  if (value.trim().length < 1 || value.length > 100) {
    return "Category name must be 1-100 characters.";
  }

  return null;
}

export function validateRecordName(value: string): string | null {
  if (value.trim().length < 1 || value.length > 255) {
    return "Record name must be 1-255 characters.";
  }

  return null;
}

export function validateSearchTerm(value: string): string | null {
  if (value.trim().length > 0 && value.length > 100) {
    return "Search term must be 1-100 characters.";
  }

  return null;
}

export function validateDate(value: string): string | null {
  if (!DATE_PATTERN.test(value)) {
    return "Date must use YYYY-MM-DD.";
  }

  return null;
}

export function validateAmount(value: number): string | null {
  if (!Number.isFinite(value)) {
    return "Amount must be a number.";
  }

  if (value === 0) {
    return "Amount cannot be 0.";
  }

  return null;
}

export function validateNickname(value: string): string | null {
  if (value.length > 50) {
    return "Nickname must be 50 characters or fewer.";
  }

  return null;
}

export function validateFriendSearchQuery(value: string): string | null {
  const length = value.trim().length;

  if (length < 1) {
    return "Search query is required.";
  }

  if (length > 50) {
    return "Search query must be 50 characters or fewer.";
  }

  return null;
}

export function validateSplitParticipantAmount(value: number): string | null {
  if (!Number.isFinite(value) || value <= 0) {
    return "Amount must be greater than 0.";
  }

  return null;
}

export function validateSplitTotals(sum: number, total: number): string | null {
  if (Math.round(sum * 100) <= Math.round(total * 100)) {
    return null;
  }

  return "Participant shares cannot exceed total amount.";
}
