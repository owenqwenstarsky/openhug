"use client";

import { useEffect, useState } from "react";
import {
  Box, Check, CircleAlert, Clock, Copy, Database, Download, File, FilePlus, FolderGit2,
  Globe, History, Lock, Pencil, Trash2,
} from "lucide-react";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useCopied } from "@/hooks/use-copied";
import { api, createRepoCommit } from "@/lib/api";
import { canWriteRepo } from "@/lib/authz";
import { isEditableTextFile } from "@/lib/files";
import { formatBytes, formatDate, timeAgo } from "@/lib/format";
import {
  encodeRepoPath, repoBlobUrl, repoEditUrl, repoNewFileUrl, type RepoRoute,
} from "@/lib/repo-routing";
import type { Commit, Repo, User } from "@/lib/types";

export function RepositoryPage({ route, navigate, user }: {
  route: Extract<RepoRoute, { page: "repo" }>; navigate: (p: string) => void; user: User | null;
}) {
  const { kind, owner, name, base } = route;
  const dataset = kind === "dataset";

  const [repo, setRepo] = useState<Repo | null>(null);
  const [failed, setFailed] = useState(false);
  const [tab, setTab] = useState<"files" | "history">("files");
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [copied, copy] = useCopied();
  const [actionError, setActionError] = useState("");

  const refreshRepo = async () => {
    const next = await api<Repo>(`/repositories/${kind}/${owner}/${name}`);
    setRepo(next);
    setCommits(null);
    return next;
  };

  useEffect(() => {
    api<Repo>(`/repositories/${kind}/${owner}/${name}`)
      .then(setRepo)
      .catch(() => setFailed(true));
  }, [kind, owner, name]);

  useEffect(() => {
    if (tab === "history" && commits === null)
      api<Commit[]>(`/repositories/${kind}/${owner}/${name}/commits`).then(setCommits).catch(() => setCommits([]));
  }, [tab, kind, owner, name, commits]);

  if (failed)
    return (
      <div className="empty empty-dashed enter">
        <span className="empty-icon"><CircleAlert /></span>
        <strong>Repository not found</strong>
        <p>It may have been deleted, or you don't have access to it.</p>
        <Button size="sm" variant="outline" onClick={() => navigate(dataset ? "/datasets" : "/models")}>
          Back to {dataset ? "datasets" : "models"}
        </Button>
      </div>
    );

  if (!repo) return <div className="page-loading"><div className="loader" /></div>;

  const uploadCommand = `openhug upload ${owner}/${name} ./files --kind ${kind}`;
  const canWrite = canWriteRepo(user, repo);

  const deleteFile = async (filePath: string) => {
    if (!canWrite) return;
    if (!confirm(`Delete ${filePath}? This creates a new commit.`)) return;
    setActionError("");
    try {
      await createRepoCommit(kind, owner, name, {
        message: `Delete ${filePath}`,
        deletions: [filePath],
      });
      await refreshRepo();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Delete failed");
    }
  };

  return (
    <section className="enter">
      <div className="breadcrumbs">
        {dataset ? <Database /> : <Box />}
        {dataset ? "Datasets" : "Models"}
        <span className="sep">/</span>
        {owner}
        <span className="sep">/</span>
        {name}
      </div>
      <div className="repo-head">
        <div>
          <h1>{name}</h1>
          <p className="sub">{repo.description || "No description yet."}</p>
        </div>
        <span className={`pill ${repo.visibility === "private" ? "pill-private" : ""}`}>
          {repo.visibility === "private" ? <Lock /> : <Globe />}{repo.visibility}
        </span>
      </div>
      <div className="repo-stats">
        <span className="meta-item"><Download />{repo.download_count} downloads</span>
        <span className="meta-item" title={formatDate(repo.updated_at)}><Clock />Updated {timeAgo(repo.updated_at)}</span>
        {repo.head_commit_id && <span className="hash-chip">{repo.head_commit_id.slice(0, 8)}</span>}
      </div>

      <div className="repo-toolbar">
        <div className="tabs">
          <button className={tab === "files" ? "active" : ""} onClick={() => setTab("files")}>
            <FolderGit2 />Files{repo.files ? ` · ${repo.files.length}` : ""}
          </button>
          <button className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}>
            <History />History
          </button>
        </div>
        <div className="repo-toolbar-actions">
          {canWrite && tab === "files" && (
            <Button size="sm" onClick={() => navigate(repoNewFileUrl(base))}>
              <FilePlus />Add file
            </Button>
          )}
          <Button size="sm" variant="outline" onClick={() => copy(uploadCommand)}>
            {copied ? <Check /> : <Copy />}{copied ? "Copied" : "Copy upload command"}
          </Button>
        </div>
      </div>

      {actionError && <Alert kind="error">{actionError}</Alert>}

      {tab === "files" ? (
        <div className="card file-table">
          {repo.files && repo.files.length > 0 && (
            <div className={`file-header ${canWrite ? "file-header-writable" : ""}`}>
              <span>Name</span><span>Size</span><span>Digest</span><span />
            </div>
          )}
          {repo.files?.map((file) => {
            const editable = isEditableTextFile(file.path, file.size);
            return (
              <div className={`file-row ${canWrite ? "file-row-writable" : ""}`} key={file.path}>
                {editable ? (
                  <button type="button" className="file-name file-name-btn" onClick={() => navigate(repoBlobUrl(base, file.path))}>
                    <File /><span>{file.path}</span>
                  </button>
                ) : (
                  <span className="file-name"><File /><span>{file.path}</span></span>
                )}
                <span className="file-size">{formatBytes(file.size)}</span>
                <code className="hash-chip">{file.sha256.slice(0, 10)}</code>
                <div className="file-actions">
                  {canWrite && editable && (
                    <button type="button" className="icon-btn" title={`Edit ${file.path}`} aria-label={`Edit ${file.path}`}
                      onClick={() => navigate(repoEditUrl(base, file.path))}>
                      <Pencil />
                    </button>
                  )}
                  {canWrite && (
                    <button type="button" className="icon-btn danger" title={`Delete ${file.path}`} aria-label={`Delete ${file.path}`}
                      onClick={() => deleteFile(file.path)}>
                      <Trash2 />
                    </button>
                  )}
                  <a className="icon-btn" title={`Download ${file.path}`} aria-label={`Download ${file.path}`}
                    href={`/api/v1/repositories/${kind}/${owner}/${name}/resolve/main/${encodeRepoPath(file.path)}`}>
                    <Download />
                  </a>
                </div>
              </div>
            );
          })}
          {(!repo.files || repo.files.length === 0) && (
            <div className="empty">
              <span className="empty-icon"><FolderGit2 /></span>
              <strong>This repository is empty</strong>
              <p>Push the first commit from your machine with the OpenHug CLI, or add a small text file here.</p>
              <code className="snippet">{uploadCommand}</code>
              {canWrite && (
                <Button size="sm" onClick={() => navigate(repoNewFileUrl(base))} style={{ marginTop: 16 }}>
                  <FilePlus />Add a text file
                </Button>
              )}
            </div>
          )}
        </div>
      ) : (
        <div className="card history-list">
          {commits === null && <div className="page-loading" style={{ minHeight: 160 }}><div className="loader" /></div>}
          {commits?.map((commit) => (
            <div className="history-row" key={commit.id}>
              <span className="commit-dot" />
              <span className="commit-main">
                <strong>{commit.message}</strong>
                <small>{commit.author} · {new Date(commit.created_at).toLocaleString()}</small>
              </span>
              <code className="hash-chip">{commit.id.slice(0, 8)}</code>
            </div>
          ))}
          {commits?.length === 0 && (
            <div className="empty">
              <span className="empty-icon"><History /></span>
              <strong>No commits yet</strong>
              <p>Upload files to create the first immutable revision.</p>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
