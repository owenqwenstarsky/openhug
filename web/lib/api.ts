import type { BlobReceipt } from "@/lib/types";
import { decodeUtf8OrThrow } from "@/lib/files";
import { encodeRepoPath } from "@/lib/repo-routing";

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    credentials: "include",
    headers: { "Content-Type": "application/json", ...init?.headers },
    ...init,
  });
  if (!response.ok) {
    const data = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(data.error || "Request failed");
  }
  if (response.status === 204) return undefined as T;
  return response.json();
}

export async function uploadBlob(bytes: Uint8Array): Promise<BlobReceipt> {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  const response = await fetch("/api/v1/blobs", {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/octet-stream" },
    body: new Blob([copy]),
  });
  if (!response.ok) {
    const data = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(data.error || "Upload failed");
  }
  return response.json();
}

export async function createRepoCommit(
  kind: string,
  owner: string,
  name: string,
  input: { message: string; files?: { path: string; sha256: string; size: number }[]; deletions?: string[] },
): Promise<void> {
  await api(`/repositories/${kind}/${owner}/${name}/commits`, {
    method: "POST",
    body: JSON.stringify({
      message: input.message,
      files: input.files ?? [],
      deletions: input.deletions ?? [],
    }),
  });
}

export async function fetchRepoTextFile(kind: string, owner: string, name: string, filePath: string): Promise<string> {
  const response = await fetch(
    `/api/v1/repositories/${kind}/${owner}/${name}/resolve/main/${encodeRepoPath(filePath)}?preview=1`,
    { credentials: "include" },
  );
  if (!response.ok) {
    const data = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(data.error || "Failed to load file");
  }
  return decodeUtf8OrThrow(await response.arrayBuffer());
}
