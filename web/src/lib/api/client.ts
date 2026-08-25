import { getApiBaseUrl } from "$lib/config";
import type { ApiError } from "$lib/api/errors";

type QueryValue = string | number | boolean | undefined | null | Array<string | number>;

type RequestOptions = {
  method?: string;
  body?: unknown;
  query?: Record<string, QueryValue>;
  headers?: Record<string, string>;
};

function buildQueryString(query?: Record<string, QueryValue>): string {
  if (!query) {
    return "";
  }

  const params = Object.entries(query)
    .filter(([, value]) => value !== undefined && value !== null)
    .map(([key, value]) => {
      const queryValue = Array.isArray(value) ? value.join(",") : String(value);
      return `${encodeURIComponent(key)}=${encodeURIComponent(queryValue)}`;
    });

  return params.length ? `?${params.join("&")}` : "";
}

async function parseErrorMessage(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { message?: string; error?: string };
    return body.message || body.error || res.statusText;
  } catch {
    return res.statusText;
  }
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> =
    options.body !== undefined ? { "Content-Type": "application/json" } : {};
  const res = await fetch(`${getApiBaseUrl()}${path}${buildQueryString(options.query)}`, {
    method: options.method,
    credentials: "include",
    headers: { ...headers, ...options.headers },
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  });

  if (!res.ok) {
    const msg = await parseErrorMessage(res);
    const err = new Error(msg) as ApiError;
    err.status = res.status;
    throw err;
  }

  if (res.status === 204) {
    return undefined as T;
  }

  const text = await res.text();
  if (!text) {
    return undefined as T;
  }

  return JSON.parse(text) as T;
}

export const client = {
  get<T>(path: string, query?: Record<string, QueryValue>, headers?: Record<string, string>) {
    return request<T>(path, { method: "GET", query, headers });
  },
  post<T>(path: string, body?: unknown, headers?: Record<string, string>) {
    return request<T>(path, { method: "POST", body, headers });
  },
  put<T>(path: string, body?: unknown, headers?: Record<string, string>) {
    return request<T>(path, { method: "PUT", body, headers });
  },
  patch<T>(path: string, body?: unknown, headers?: Record<string, string>) {
    return request<T>(path, { method: "PATCH", body, headers });
  },
  del<T>(path: string, headers?: Record<string, string>) {
    return request<T>(path, { method: "DELETE", headers });
  },
};
