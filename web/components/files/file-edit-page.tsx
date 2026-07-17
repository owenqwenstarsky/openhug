"use client";

import { useEffect, useState } from "react";
import { CircleAlert, Lock } from "lucide-react";
import { FilePageShell } from "@/components/files/file-page-shell";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { api, createRepoCommit, fetchRepoTextFile, uploadBlob } from "@/lib/api";
import { canWriteRepo } from "@/lib/authz";
import { isEditableTextFile, TEXT_EDIT_MAX_BYTES } from "@/lib/files";
import { formatBytes } from "@/lib/format";
import { repoBlobUrl, type RepoRoute } from "@/lib/repo-routing";
import type { Repo, User } from "@/lib/types";

export function FileEditPage({ route, navigate, user }: {
  route: Extract<RepoRoute, { page: "edit" }>; navigate: (p: string) => void; user: User | null;
}) {
  const { kind, owner, name, base, filePath } = route;
  const [repo, setRepo] = useState<Repo | null>(null);
  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    Promise.all([
      api<Repo>(`/repositories/${kind}/${owner}/${name}`),
      fetchRepoTextFile(kind, owner, name, filePath),
    ])
      .then(([nextRepo, text]) => {
        if (cancelled) return;
        setRepo(nextRepo);
        setContent(text);
        setOriginal(text);
      })
      .catch((err: Error) => {
        if (!cancelled) setError(err.message || "Failed to load file");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [kind, owner, name, filePath]);

  const canWrite = repo ? canWriteRepo(user, repo) : false;
  const dirty = content !== original;

  const save = async () => {
    if (!canWrite) return;
    const encoded = new TextEncoder().encode(content);
    if (encoded.byteLength > TEXT_EDIT_MAX_BYTES) {
      setSaveError(`File exceeds the ${formatBytes(TEXT_EDIT_MAX_BYTES)} text edit limit`);
      return;
    }
    if (!isEditableTextFile(filePath, encoded.byteLength)) {
      setSaveError("Only small text files can be edited in the browser");
      return;
    }
    setBusy(true);
    setSaveError("");
    try {
      const receipt = await uploadBlob(encoded);
      await createRepoCommit(kind, owner, name, {
        message: `Update ${filePath}`,
        files: [{ path: filePath, sha256: receipt.sha256, size: receipt.size }],
      });
      navigate(repoBlobUrl(base, filePath));
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
        <strong>Could not open file</strong>
        <p>{error}</p>
        <Button size="sm" variant="outline" onClick={() => navigate(base)}>Back to repository</Button>
      </div>
    );
  }
  if (!canWrite) {
    return (
      <div className="empty empty-dashed enter">
        <span className="empty-icon"><Lock /></span>
        <strong>Editing requires write access</strong>
        <p>Only the repository owner can edit files here.</p>
        <Button size="sm" variant="outline" onClick={() => navigate(repoBlobUrl(base, filePath))}>View file</Button>
      </div>
    );
  }

  return (
    <FilePageShell
      route={route}
      filePath={filePath}
      navigate={navigate}
      actions={
        <>
          <Button size="sm" variant="outline" onClick={() => navigate(repoBlobUrl(base, filePath))} disabled={busy}>
            Cancel
          </Button>
          <Button size="sm" onClick={save} disabled={busy || !dirty}>
            {busy ? "Saving…" : "Save"}
          </Button>
        </>
      }
    >
      {saveError && <Alert kind="error">{saveError}</Alert>}
      <div className="card file-editor">
        <textarea
          className="file-editor-textarea"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          spellCheck={false}
          autoFocus
        />
      </div>
    </FilePageShell>
  );
}
