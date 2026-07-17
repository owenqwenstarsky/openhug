"use client";

import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { CircleAlert, Download, Pencil, Trash2 } from "lucide-react";
import { FilePageShell } from "@/components/files/file-page-shell";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { api, createRepoCommit, fetchRepoTextFile } from "@/lib/api";
import { canWriteRepo } from "@/lib/authz";
import { isMarkdownFile } from "@/lib/files";
import { encodeRepoPath, repoEditUrl, type RepoRoute } from "@/lib/repo-routing";
import type { Repo, User } from "@/lib/types";

export function FileViewPage({ route, navigate, user }: {
  route: Extract<RepoRoute, { page: "blob" }>; navigate: (p: string) => void; user: User | null;
}) {
  const { kind, owner, name, base, filePath } = route;
  const [repo, setRepo] = useState<Repo | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [actionError, setActionError] = useState("");

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
  const markdown = isMarkdownFile(filePath);

  const deleteFile = async () => {
    if (!canWrite) return;
    if (!confirm(`Delete ${filePath}? This creates a new commit.`)) return;
    setActionError("");
    try {
      await createRepoCommit(kind, owner, name, {
        message: `Delete ${filePath}`,
        deletions: [filePath],
      });
      navigate(base);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Delete failed");
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

  return (
    <FilePageShell
      route={route}
      filePath={filePath}
      navigate={navigate}
      actions={
        <>
          {canWrite && (
            <Button size="sm" onClick={() => navigate(repoEditUrl(base, filePath))}>
              <Pencil />Edit
            </Button>
          )}
          {canWrite && (
            <Button size="sm" variant="outline" onClick={deleteFile}>
              <Trash2 />Delete
            </Button>
          )}
          <Button size="sm" variant="outline" asChild>
            <a href={`/api/v1/repositories/${kind}/${owner}/${name}/resolve/main/${encodeRepoPath(filePath)}`}>
              <Download />Download
            </a>
          </Button>
        </>
      }
    >
      {actionError && <Alert kind="error">{actionError}</Alert>}
      <div className="card file-view">
        {markdown ? (
          <div className="markdown-body">
            <ReactMarkdown>{content}</ReactMarkdown>
          </div>
        ) : (
          <pre className="file-view-pre">{content}</pre>
        )}
      </div>
    </FilePageShell>
  );
}
