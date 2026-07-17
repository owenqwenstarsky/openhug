"use client";

import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight, Box, ChevronDown, Clock, Database, Download, Globe, Lock, Plus, Search, X,
} from "lucide-react";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate, timeAgo } from "@/lib/format";
import type { Repo } from "@/lib/types";

type SortKey = "updated" | "name" | "downloads";
const SORT_OPTIONS: [SortKey, string][] = [
  ["updated", "Recently updated"],
  ["name", "Name (A–Z)"],
  ["downloads", "Most downloads"],
];

export function Explore({ kind, navigate }: { kind: "model" | "dataset"; navigate: (p: string) => void }) {
  const [repos, setRepos] = useState<Repo[] | null>(null);
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [sort, setSort] = useState<SortKey>("updated");
  const [error, setError] = useState("");

  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), 250);
    return () => clearTimeout(t);
  }, [query]);

  useEffect(() => { setRepos(null); }, [kind]);

  useEffect(() => {
    let alive = true;
    setError("");
    api<Repo[]>(`/repositories?kind=${kind}&search=${encodeURIComponent(debounced)}&limit=100`)
      .then((r) => { if (alive) setRepos(r); })
      .catch((e) => { if (alive) { setError((e as Error).message); setRepos([]); } });
    return () => { alive = false; };
  }, [kind, debounced]);

  const sorted = useMemo(() => {
    if (!repos) return [];
    const list = [...repos];
    if (sort === "updated") list.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
    if (sort === "name") list.sort((a, b) => a.name.localeCompare(b.name));
    if (sort === "downloads") list.sort((a, b) => b.download_count - a.download_count);
    return list;
  }, [repos, sort]);

  const plural = kind === "model" ? "Models" : "Datasets";

  return (
    <section className="enter">
      <div className="page-head">
        <div>
          <p className="eyebrow">Registry / {plural}</p>
          <h1>{plural}</h1>
          <p className="sub">
            {kind === "model"
              ? "Weights, configurations, and the context to run them."
              : "Versioned data with provenance you can inspect."}
          </p>
        </div>
        <span className="count-chip">
          <strong>{repos ? repos.length : "–"}</strong>
          <span>{repos?.length === 1 ? "repository" : "repositories"}</span>
        </span>
      </div>

      <div className="toolbar">
        <div className="search-box">
          <Search />
          <Input placeholder={`Search ${plural.toLowerCase()}…`} value={query} onChange={(e) => setQuery(e.target.value)} />
          {query && (
            <button className="search-clear" onClick={() => setQuery("")} aria-label="Clear search"><X /></button>
          )}
        </div>
        <span className="spacer" />
        <label className="sort-select" aria-label="Sort repositories">
          <select value={sort} onChange={(e) => setSort(e.target.value as SortKey)}>
            {SORT_OPTIONS.map(([value, label]) => (
              <option key={value} value={value}>{label}</option>
            ))}
          </select>
          <ChevronDown />
        </label>
      </div>

      {error && (
        <Alert kind="error">
          Couldn't load {plural.toLowerCase()}: {error}
        </Alert>
      )}

      {!error && repos === null && (
        <div className="card repo-list">
          {Array.from({ length: 5 }).map((_, i) => (
            <div className="repo-row" key={i} style={{ cursor: "default" }}>
              <span className="skeleton" style={{ width: 38, height: 38, borderRadius: 10 }} />
              <span className="repo-main">
                <span className="skeleton" style={{ width: "38%", height: 14 }} />
                <span className="skeleton" style={{ width: "62%", height: 11 }} />
              </span>
              <span className="skeleton" style={{ width: 70, height: 20, borderRadius: 999 }} />
            </div>
          ))}
        </div>
      )}

      {!error && repos !== null && sorted.length > 0 && (
        <div className="card repo-list">
          {sorted.map((repo, i) => (
            <button
              className="repo-row"
              style={{ animationDelay: `${i * 25}ms` }}
              key={repo.id}
              onClick={() => navigate(repo.kind === "dataset" ? `/datasets/${repo.owner}/${repo.name}` : `/${repo.owner}/${repo.name}`)}
            >
              <span className={`kind-icon kind-${repo.kind}`}>
                {repo.kind === "model" ? <Box /> : <Database />}
              </span>
              <span className="repo-main">
                <span className="repo-name">
                  <span className="owner">{repo.owner}</span>/<span className="slug">{repo.name}</span>
                </span>
                <span className="repo-desc">{repo.description || "No description yet."}</span>
              </span>
              <span className="repo-meta">
                <span className="meta-item"><Download />{repo.download_count}</span>
                <span className="meta-item hide-mobile" title={formatDate(repo.updated_at)}>
                  <Clock />{timeAgo(repo.updated_at)}
                </span>
                <span className={`pill ${repo.visibility === "private" ? "pill-private" : ""}`}>
                  {repo.visibility === "private" ? <Lock /> : <Globe />}{repo.visibility}
                </span>
                <ArrowRight className="row-arrow" />
              </span>
            </button>
          ))}
        </div>
      )}

      {!error && repos !== null && sorted.length === 0 && (
        <div className="empty empty-dashed">
          <span className="empty-icon">{kind === "model" ? <Box /> : <Database />}</span>
          {debounced ? (
            <>
              <strong>No matches for “{debounced}”</strong>
              <p>Try a different name or keyword.</p>
              <Button size="sm" variant="outline" onClick={() => setQuery("")}>Clear search</Button>
            </>
          ) : (
            <>
              <strong>No {plural.toLowerCase()} yet</strong>
              <p>Create the first {kind} repository on this instance, then push files with the CLI.</p>
              <Button size="sm" onClick={() => navigate(`/new/${kind}`)}><Plus />New {kind}</Button>
            </>
          )}
        </div>
      )}
    </section>
  );
}
