"use client";

import { useEffect, useState } from "react";
import { CircleAlert, Lock } from "lucide-react";
import { FilePageShell } from "@/components/files/file-page-shell";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, createRepoCommit, uploadBlob } from "@/lib/api";
import { canWriteRepo } from "@/lib/authz";
import { isEditableTextFile, TEXT_EDIT_MAX_BYTES } from "@/lib/files";
import { formatBytes } from "@/lib/format";
import { repoBlobUrl, type RepoRoute } from "@/lib/repo-routing";
import type { Repo, User } from "@/lib/types";

export function FileCreatePage({ route, navigate, user }: {
  route: Extract<RepoRoute, { page: "new-file" }>; navigate: (p: string) => void; user: User | null;
}) {
  const { kind, owner, name, base } = route;
  const [repo, setRepo] = useState<Repo | null>(null);
  const [filePath, setFilePath] = useState("");
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    api<Repo>(`/repositories/${kind}/${owner}/${name}`)
      .then(setRepo)
      .catch((err: Error) => setError(err.message || "Repository not found"))
      .finally(() => setLoading(false));
  }, [kind, owner, name]);

  const canWrite = repo ? canWriteRepo(user, repo) : false;
  const dirty = filePath.trim().length > 0 || content.length > 0;

  const save = async () => {
    if (!canWrite) return;
    const targetPath = filePath.trim();
    if (!targetPath) {
      setSaveError("Enter a file path");
      return;
    }
    const encoded = new TextEncoder().encode(content);
    if (encoded.byteLength > TEXT_EDIT_MAX_BYTES) {
      setSaveError(`File exceeds the ${formatBytes(TEXT_EDIT_MAX_BYTES)} text edit limit`);
      return;
    }
    if (!isEditableTextFile(targetPath, encoded.byteLength)) {
      setSaveError("Only small text files can be created in the browser");
      return;
    }
    setBusy(true);
    setSaveError("");
    try {
      const receipt = await uploadBlob(encoded);
      await createRepoCommit(kind, owner, name, {
        message: `Create ${targetPath}`,
        files: [{ path: targetPath, sha256: receipt.sha256, size: receipt.size }],
      });
      navigate(repoBlobUrl(base, targetPath));
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setBusy(false);
    }
  };

  if (loading) return <div className="page-loading"><div className="loader" /></div>;
  if (error) {
    return (
      <div className="empty empty-dashed enter">
        <span className="empty-icon"><CircleAlert /></span>
        <strong>Repository not found</strong>
        <p>{error}</p>
        <Button size="sm" variant="outline" onClick={() => navigate("/models")}>Back to models</Button>
      </div>
    );
  }
  if (!canWrite) {
    return (
      <div className="empty empty-dashed enter">
        <span className="empty-icon"><Lock /></span>
        <strong>Creating files requires write access</strong>
        <p>Only the repository owner can add files here.</p>
        <Button size="sm" variant="outline" onClick={() => navigate(base)}>Back to repository</Button>
      </div>
    );
  }

  return (
    <FilePageShell
      route={route}
      navigate={navigate}
      actions={
        <>
          <Button size="sm" variant="outline" onClick={() => navigate(base)} disabled={busy}>
            Cancel
          </Button>
          <Button size="sm" onClick={save} disabled={busy || !dirty}>
            {busy ? "Creating…" : "Create file"}
          </Button>
        </>
      }
    >
      {saveError && <Alert kind="error">{saveError}</Alert>}
      <div className="card file-editor">
        <div className="file-editor-head">
          <div className="file-editor-title">
            <Input
              autoFocus
              placeholder="path/to/file.md"
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
              disabled={busy}
            />
            <small>New text file · up to {formatBytes(TEXT_EDIT_MAX_BYTES)}</small>
          </div>
        </div>
        <textarea
          className="file-editor-textarea"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          spellCheck={false}
          placeholder="File contents…"
        />
      </div>
    </FilePageShell>
  );
}
