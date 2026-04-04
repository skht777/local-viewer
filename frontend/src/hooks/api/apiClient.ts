// API クライアント (fetch ラッパー)
// - Vite proxy 経由で /api にアクセス
// - エラー時は ApiError を throw

export class ApiError extends Error {
  constructor(
    public status: number,
    public body: { error: string; code: string; detail?: string },
  ) {
    super(body.error);
    this.name = "ApiError";
  }
}

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, init);
  if (!response.ok) {
    const body = await response.json();
    throw new ApiError(response.status, body);
  }
  return response.json() as Promise<T>;
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  return apiFetch<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}
