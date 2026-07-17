export type RepoRoute =
  | { kind: "model" | "dataset"; owner: string; name: string; base: string; page: "repo" }
  | { kind: "model" | "dataset"; owner: string; name: string; base: string; page: "blob"; filePath: string }
  | { kind: "model" | "dataset"; owner: string; name: string; base: string; page: "edit"; filePath: string }
  | { kind: "model" | "dataset"; owner: string; name: string; base: string; page: "new-file" };

export function encodeRepoPath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

export function decodeRepoPath(segments: string[]): string {
  return segments.map((s) => {
    try {
      return decodeURIComponent(s);
    } catch {
      return s;
    }
  }).join("/");
}

export function parseRepoRoute(path: string): RepoRoute | null {
  const parts = path.split("/").filter(Boolean);
  if (parts.length < 2) return null;

  let kind: "model" | "dataset";
  let owner: string;
  let name: string;
  let rest: string[];

  if (parts[0] === "datasets") {
    if (parts.length < 3) return null;
    kind = "dataset";
    owner = parts[1];
    name = parts[2];
    rest = parts.slice(3);
  } else {
    if (["models", "settings", "new"].includes(parts[0])) return null;
    kind = "model";
    owner = parts[0];
    name = parts[1];
    rest = parts.slice(2);
  }

  const base = kind === "dataset" ? `/datasets/${owner}/${name}` : `/${owner}/${name}`;
  if (rest.length === 0) return { kind, owner, name, base, page: "repo" };
  if (rest[0] === "new" && rest[1] === "file" && rest.length === 2) {
    return { kind, owner, name, base, page: "new-file" };
  }
  if (rest[0] === "blob" && rest[1] === "main" && rest.length >= 3) {
    return { kind, owner, name, base, page: "blob", filePath: decodeRepoPath(rest.slice(2)) };
  }
  if (rest[0] === "edit" && rest[1] === "main" && rest.length >= 3) {
    return { kind, owner, name, base, page: "edit", filePath: decodeRepoPath(rest.slice(2)) };
  }
  return null;
}

export function repoBlobUrl(base: string, filePath: string): string {
  return `${base}/blob/main/${encodeRepoPath(filePath)}`;
}

export function repoEditUrl(base: string, filePath: string): string {
  return `${base}/edit/main/${encodeRepoPath(filePath)}`;
}

export function repoNewFileUrl(base: string): string {
  return `${base}/new/file`;
}
