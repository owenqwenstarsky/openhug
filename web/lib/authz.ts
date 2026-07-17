import type { Repo, ThemeMode, User } from "@/lib/types";

export function canWriteRepo(user: User | null | undefined, repo: Repo): boolean {
  return Boolean(user && (user.username === repo.owner || user.role === "superuser"));
}

export function normalizeTheme(theme?: string): ThemeMode {
  return theme === "dark" ? "dark" : "light";
}
