import { goto } from "$app/navigation";

export type ApiError = Error & { status?: number };

export function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  return fallback;
}

export async function handleApiError(error: unknown, fallback: string): Promise<string> {
  if ((error as ApiError)?.status === 401) {
    await goto("/login");
    return "";
  }

  return getErrorMessage(error, fallback);
}
