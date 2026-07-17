export const TEXT_EDIT_MAX_BYTES = 1024 * 1024;
export const TEXT_EXTENSIONS = new Set([
  ".md", ".txt", ".json", ".yaml", ".yml", ".toml", ".csv", ".tsv",
  ".py", ".rs", ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs",
  ".css", ".scss", ".html", ".htm", ".xml", ".svg",
  ".ini", ".cfg", ".conf", ".sh", ".bash", ".zsh", ".env",
  ".sql", ".r", ".rb", ".go", ".java", ".kt", ".swift", ".c", ".h", ".cpp", ".hpp",
  ".proto", ".graphql", ".lock", ".log",
]);
export const TEXT_BASENAMES = new Set([
  "readme", "license", "licence", "copying", "authors", "contributors",
  "changelog", "changes", "dockerfile", "makefile", "gemfile", "procfile",
  ".gitignore", ".gitattributes", ".dockerignore", ".editorconfig", ".npmrc",
]);

export function textBasename(path: string): string {
  return path.split("/").pop()?.toLowerCase() ?? "";
}

export function isEditableTextFile(path: string, size: number): boolean {
  if (size > TEXT_EDIT_MAX_BYTES || path.length === 0) return false;
  const base = textBasename(path);
  if (TEXT_BASENAMES.has(base)) return true;
  const dot = base.lastIndexOf(".");
  if (dot <= 0) return TEXT_BASENAMES.has(base);
  return TEXT_EXTENSIONS.has(base.slice(dot));
}

export function decodeUtf8OrThrow(bytes: ArrayBuffer): string {
  const view = new Uint8Array(bytes);
  if (view.includes(0)) throw new Error("File contains binary data");
  const decoder = new TextDecoder("utf-8", { fatal: true });
  try {
    return decoder.decode(view);
  } catch {
    throw new Error("File is not valid UTF-8 text");
  }
}

export function isMarkdownFile(path: string): boolean {
  const base = textBasename(path);
  return base.endsWith(".md") || base.endsWith(".markdown");
}
