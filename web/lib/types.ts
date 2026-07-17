export type ThemeMode = "light" | "dark";
export type User = { id: string; username: string; email: string; role: string; theme: ThemeMode };
export type Repo = {
  id: string; owner: string; kind: "model" | "dataset"; name: string; description: string;
  visibility: "public" | "private"; head_commit_id?: string; download_count: number;
  updated_at: string; files?: RepoFile[];
};
export type RepoFile = { path: string; sha256: string; size: number };
export type Token = { id: string; name: string; scopes: string[]; created_at: string; last_used_at: string | null };
export type Commit = { id: string; author: string; message: string; created_at: string };
export type AdminSettings = { instance_name: string; signup_policy: string; default_visibility: string; retention_days: number };
export type AdminUser = { id: string; username: string; email: string; role: string; status: string };
export type BlobReceipt = { sha256: string; size: number };
