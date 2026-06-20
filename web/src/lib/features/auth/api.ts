import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type PublicUser = components["schemas"]["PublicUser"];

export async function getMe(): Promise<PublicUser | null> {
  try {
    return await client.get<PublicUser>("/auth/me");
  } catch (error) {
    if ((error as { status?: number }).status === 401) {
      return null;
    }

    throw error;
  }
}

export function register(username: string, password: string): Promise<PublicUser> {
  return client.post<PublicUser>("/auth/register", { username, password });
}

export function login(username: string, password: string): Promise<PublicUser> {
  return client.post<PublicUser>("/auth/login", { username, password });
}

export function logout(): Promise<void> {
  return client.post<void>("/auth/logout");
}
