"use client";

import { FormEvent, ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowRight, Box, Check, ChevronDown, CircleAlert, Clock, Copy, Database, Download, File,
  FolderGit2, Globe, History, KeyRound, Lock, LogOut, Moon, Plus, Search, Settings, ShieldCheck,
  Sun, Trash2, X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type ThemeMode = "light" | "dark";
type User = { id: string; username: string; email: string; role: string; theme: ThemeMode };
type Repo = {
  id: string; owner: string; kind: "model" | "dataset"; name: string; description: string;
  visibility: "public" | "private"; head_commit_id?: string; download_count: number;
  updated_at: string; files?: RepoFile[];
};
type RepoFile = { path: string; sha256: string; size: number };
type Token = { id: string; name: string; scopes: string[]; created_at: string; last_used_at: string | null };
type Commit = { id: string; author: string; message: string; created_at: string };
type AdminSettings = { instance_name: string; signup_policy: string; default_visibility: string; retention_days: number };
type AdminUser = { id: string; username: string; email: string; role: string; status: string };

async function api<T>(path: string, init?: RequestInit): Promise<T> {
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

function normalizeTheme(theme?: string): ThemeMode {
  return theme === "dark" ? "dark" : "light";
}

/* ---------------------------------------------------------------- helpers */

function timeAgo(iso?: string | null): string {
  if (!iso) return "never";
  const seconds = (Date.now() - new Date(iso).getTime()) / 1000;
  if (seconds < 45) return "just now";
  const minutes = seconds / 60;
  if (minutes < 60) return `${Math.floor(minutes)}m ago`;
  const hours = minutes / 60;
  if (hours < 24) return `${Math.floor(hours)}h ago`;
  const days = hours / 24;
  if (days < 30) return `${Math.floor(days)}d ago`;
  const months = days / 30;
  if (months < 12) return `${Math.floor(months)}mo ago`;
  return `${Math.floor(months / 12)}y ago`;
}

function formatDate(iso?: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1048576) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1073741824) return `${(size / 1048576).toFixed(1)} MB`;
  return `${(size / 1073741824).toFixed(1)} GB`;
}

function useCopied(timeout = 1600): [boolean, (text: string) => void] {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(null);
  return [
    copied,
    (text) => {
      navigator.clipboard.writeText(text);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), timeout);
    },
  ];
}

/* ---------------------------------------------------------------- root app */

export default function App() {
  const [ready, setReady] = useState(false);
  const [initialized, setInitialized] = useState(true);
  const [instance, setInstance] = useState("OpenHug");
  const [signupPolicy, setSignupPolicy] = useState("disabled");
  const [defaultVisibility, setDefaultVisibility] = useState<"public" | "private">("public");
  const [setupTokenRequired, setSetupTokenRequired] = useState(false);
  const [user, setUser] = useState<User | null>(null);
  const [path, setPath] = useState("/");

  useEffect(() => {
    document.documentElement.dataset.theme = normalizeTheme(user?.theme);
    document.documentElement.style.colorScheme = normalizeTheme(user?.theme);
  }, [user?.theme]);

  useEffect(() => {
    setPath(location.pathname);
    api<{ initialized: boolean; instance_name?: string; signup_policy?: string; default_visibility?: "public" | "private"; setup_token_required?: boolean }>("/setup/status")
      .then((s) => {
        setInitialized(s.initialized);
        setInstance(s.instance_name || "OpenHug");
        setSignupPolicy(s.signup_policy || "disabled");
        setDefaultVisibility(s.default_visibility || "public");
        setSetupTokenRequired(Boolean(s.setup_token_required));
        if (s.initialized) return api<User>("/auth/me").then((u) => setUser({ ...u, theme: normalizeTheme(u.theme) })).catch(() => null);
      })
      .finally(() => setReady(true));
  }, []);

  const navigate = (to: string) => {
    history.pushState({}, "", to);
    setPath(to);
  };

  useEffect(() => {
    const pop = () => setPath(location.pathname);
    addEventListener("popstate", pop);
    return () => removeEventListener("popstate", pop);
  }, []);

  if (!ready) return <Splash />;
  if (!initialized)
    return (
      <Onboarding
        setupTokenRequired={setupTokenRequired}
        onDone={(name, u) => {
          setInstance(name);
          setUser(u);
          setInitialized(true);
          navigate("/models");
        }}
      />
    );
  if (!user && isRepoPath(path))
    return <PublicRepositoryPage instance={instance} path={path} navigate={navigate} onSignIn={() => navigate("/")} />;
  if (!user)
    return <Auth instance={instance} signupPolicy={signupPolicy} onLogin={(u) => { setUser({ ...u, theme: normalizeTheme(u.theme) }); navigate("/models"); }} />;

  const updateTheme = async (theme: ThemeMode) => {
    const updated = await api<User>("/auth/me", { method: "PUT", body: JSON.stringify({ theme }) });
    setUser({ ...updated, theme: normalizeTheme(updated.theme) });
  };

  return (
    <Shell
      instance={instance}
      user={user}
      path={path}
      navigate={navigate}
      defaultVisibility={defaultVisibility}
      onThemeChange={updateTheme}
      logout={async () => {
        await api("/auth/logout", { method: "POST" });
        setUser(null);
      }}
    />
  );
}

function Splash() {
  return (
    <main className="splash">
      <Brand />
      <div className="loader" />
    </main>
  );
}

function Brand({ name = "OpenHug", onNavigate }: { name?: string; onNavigate?: () => void }) {
  if (onNavigate)
    return (
      <button className="brand" onClick={onNavigate} aria-label={`${name} home`}>
        <strong>{name}</strong>
      </button>
    );
  return (
    <span className="brand">
      <strong>{name}</strong>
    </span>
  );
}

/* ------------------------------------------------------------- onboarding */

function Onboarding({ setupTokenRequired, onDone }: { setupTokenRequired: boolean; onDone: (name: string, user: User) => void }) {
  const [step, setStep] = useState(1);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [data, setData] = useState({
    instance_name: "OpenHug", username: "", email: "", password: "",
    signup_policy: "approval", default_visibility: "public", retention_days: 30, setup_token: "",
  });

  const finish = async () => {
    try {
      setError("");
      setBusy(true);
      await api("/setup", { method: "POST", body: JSON.stringify(data) });
      const user = await api<User>("/auth/me");
      onDone(data.instance_name, user);
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  return (
    <main className="onboarding">
      <div className="onboarding-aside">
        <Brand />
        <div>
          <p className="eyebrow">Your infrastructure. Your rules.</p>
          <h1>A home for the work that moves AI forward.</h1>
          <p>Models, datasets, and every revision—kept close, on storage you control.</p>
        </div>
        <ol>
          {["Instance", "Superuser", "Access"].map((x, i) => (
            <li className={step === i + 1 ? "active" : step > i + 1 ? "done" : ""} key={x}>
              <span>{step > i + 1 ? <Check /> : i + 1}</span>
              {x}
            </li>
          ))}
        </ol>
      </div>
      <section className="onboarding-form">
        <div className="form-wrap">
          <p className="step">Step {step} of 3</p>
          {step === 1 && (
            <>
              <h2>Name this place.</h2>
              <p>This is how your hub will appear to everyone on the instance.</p>
              <Field label="Instance name">
                <Input autoFocus value={data.instance_name} onChange={(e) => setData({ ...data, instance_name: e.target.value })} />
              </Field>
              <InfraNote />
            </>
          )}
          {step === 2 && (
            <>
              <h2>Create the superuser.</h2>
              <p>This account controls instance policy and user access.</p>
              <Field label="Username">
                <Input autoFocus value={data.username} onChange={(e) => setData({ ...data, username: e.target.value.toLowerCase() })} />
              </Field>
              <Field label="Email">
                <Input type="email" value={data.email} onChange={(e) => setData({ ...data, email: e.target.value })} />
              </Field>
              <Field label="Password" hint="At least 12 characters.">
                <Input type="password" minLength={12} value={data.password} onChange={(e) => setData({ ...data, password: e.target.value })} />
              </Field>
            </>
          )}
          {step === 3 && (
            <>
              <h2>Set the ground rules.</h2>
              <p>You can change application policy later in administration.</p>
              <Field label="New account access">
                <Select value={data.signup_policy} onChange={(v) => setData({ ...data, signup_policy: v })}
                  options={[["approval", "Require admin approval"], ["immediate", "Immediate access"], ["disabled", "Disable signups"]]} />
              </Field>
              <Field label="Default repository visibility">
                <Select value={data.default_visibility} onChange={(v) => setData({ ...data, default_visibility: v })}
                  options={[["public", "Public"], ["private", "Private"]]} />
              </Field>
              {setupTokenRequired && (
                <Field label="Setup token" hint="Value of OPENHUG_SETUP_TOKEN from the server environment.">
                  <Input type="password" value={data.setup_token} onChange={(e) => setData({ ...data, setup_token: e.target.value })} />
                </Field>
              )}
            </>
          )}
          {error && <Alert kind="error">{error}</Alert>}
          <div className="form-actions">
            {step > 1 && <Button variant="ghost" onClick={() => setStep(step - 1)}>Back</Button>}
            <Button onClick={() => (step < 3 ? setStep(step + 1) : finish())}
              disabled={busy || (step === 2 && (!data.username || !data.email || data.password.length < 12))}>
              {step < 3 ? "Continue" : busy ? "Initializing…" : "Initialize OpenHug"}<ArrowRight />
            </Button>
          </div>
        </div>
      </section>
    </main>
  );
}

function InfraNote() {
  return (
    <div className="infra-note">
      <Database />
      <div>
        <strong>Infrastructure connected</strong>
        <p>Database and storage configuration are loaded securely from the server environment.</p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------- auth */

function Auth({ instance, signupPolicy, onLogin }: { instance: string; signupPolicy: string; onLogin: (u: User) => void }) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [identity, setIdentity] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      setError("");
      setBusy(true);
      if (mode === "signup") {
        const result = await api<{ status: string }>("/auth/signup", {
          method: "POST",
          body: JSON.stringify({ username, email: identity, password }),
        });
        if (result.status === "pending") {
          setNotice("Your account is waiting for administrator approval.");
          setMode("login");
          setBusy(false);
          return;
        }
      }
      await api("/auth/login", { method: "POST", body: JSON.stringify({ identity, password }) });
      onLogin(await api<User>("/auth/me"));
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  return (
    <main className="auth-page">
      <header><Brand name={instance} /></header>
      <form className="auth-form" onSubmit={submit}>
        <p className="eyebrow">Private infrastructure</p>
        <h1>{mode === "login" ? "Welcome back." : "Create your account."}</h1>
        <p>{mode === "login" ? `Sign in to continue to ${instance}.` : `Join ${instance} with a local account.`}</p>
        {mode === "signup" && (
          <Field label="Username">
            <Input autoFocus value={username} onChange={(e) => setUsername(e.target.value.toLowerCase())} />
          </Field>
        )}
        <Field label={mode === "signup" ? "Email" : "Email or username"}>
          <Input autoFocus={mode === "login"} type={mode === "signup" ? "email" : "text"} value={identity} onChange={(e) => setIdentity(e.target.value)} />
        </Field>
        <Field label="Password">
          <Input type="password" minLength={mode === "signup" ? 12 : undefined} value={password} onChange={(e) => setPassword(e.target.value)} />
        </Field>
        {notice && <Alert kind="notice">{notice}</Alert>}
        {error && <Alert kind="error">{error}</Alert>}
        <Button type="submit" disabled={busy}>
          {busy ? "One moment…" : mode === "login" ? "Sign in" : "Create account"}<ArrowRight />
        </Button>
        {signupPolicy !== "disabled" && (
          <button className="auth-switch" type="button"
            onClick={() => { setMode(mode === "login" ? "signup" : "login"); setError(""); setNotice(""); }}>
            {mode === "login" ? "Need an account? Sign up" : "Already have an account? Sign in"}
          </button>
        )}
      </form>
      <div className="auth-art">
        <div className="orbit orbit-one" />
        <div className="orbit orbit-two" />
        <span className="core" />
      </div>
    </main>
  );
}

/* ------------------------------------------------------------------ shell */

function Shell({ instance, user, path, navigate, defaultVisibility, onThemeChange, logout }: {
  instance: string; user: User; path: string; navigate: (p: string) => void; defaultVisibility: "public" | "private"; onThemeChange: (theme: ThemeMode) => Promise<void>; logout: () => void;
}) {
  const link = (to: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    navigate(to);
  };

  useEffect(() => {
    const parts = path.split("/").filter(Boolean);
    let label = "Models";
    if (path === "/settings") label = "Settings";
    else if (path.startsWith("/new/")) label = "New repository";
    else if (path.startsWith("/datasets")) label = parts.length === 3 ? parts.slice(1).join("/") : "Datasets";
    else if (isRepoPath(path)) label = parts.join("/");
    document.title = `${label} · ${instance}`;
  }, [path, instance]);

  return (
    <div className="app">
      <header className="topbar">
        <div className="topbar-inner">
          <Brand name={instance} onNavigate={() => navigate("/models")} />
          <nav className="topbar-nav">
            <a className={path.startsWith("/models") || path === "/" ? "active" : ""} href="/models" onClick={link("/models")}>
              <Box /><span>Models</span>
            </a>
            <a className={path.startsWith("/datasets") ? "active" : ""} href="/datasets" onClick={link("/datasets")}>
              <Database /><span>Datasets</span>
            </a>
          </nav>
          <div className="top-actions">
            <Popover
              trigger={({ toggle }) => (
                <Button size="sm" onClick={toggle}>
                  <Plus /><span className="new-btn-label">New</span><ChevronDown />
                </Button>
              )}
            >
              {(close) => (
                <>
                  <button className="menu-item" onClick={() => { close(); navigate("/new/model"); }}>
                    <Box /><span><strong>Model</strong><small>Weights, configs, and cards</small></span>
                  </button>
                  <button className="menu-item" onClick={() => { close(); navigate("/new/dataset"); }}>
                    <Database /><span><strong>Dataset</strong><small>Data files and documentation</small></span>
                  </button>
                </>
              )}
            </Popover>
            <a aria-label="Settings" title="Settings" href="/settings" onClick={link("/settings")} className="icon-btn">
              <Settings />
            </a>
            <Popover
              trigger={({ toggle }) => (
                <button className="avatar" onClick={toggle} title={user.username} aria-label="Account menu">
                  {user.username.slice(0, 2).toUpperCase()}
                </button>
              )}
            >
              {(close) => (
                <>
                  <div className="menu-head">
                    <strong>{user.username}</strong>
                    <small>{user.email}</small>
                  </div>
                  <button className="menu-item" onClick={() => { close(); navigate("/settings"); }}>
                    <Settings />Settings
                  </button>
                  <button className="menu-item menu-toggle-item" onClick={async () => { try { await onThemeChange(user.theme === "dark" ? "light" : "dark"); close(); } catch (e) { console.error(e); } }}>
                    {user.theme === "dark" ? <Sun /> : <Moon />}
                    <span><strong>{user.theme === "dark" ? "Light mode" : "Dark mode"}</strong><small>Saved to your account</small></span>
                    <span className={`switch ${user.theme === "dark" ? "on" : ""}`} aria-hidden="true"><span /></span>
                  </button>
                  <div className="menu-sep" />
                  <button className="menu-item danger" onClick={() => { close(); logout(); }}>
                    <LogOut />Sign out
                  </button>
                </>
              )}
            </Popover>
          </div>
        </div>
      </header>
      <main className="workspace">
        {path === "/settings" ? (
          <SettingsPage user={user} onThemeChange={onThemeChange} logout={logout} />
        ) : path.startsWith("/new/") ? (
          <NewRepository kind={path.endsWith("dataset") ? "dataset" : "model"} user={user} navigate={navigate} defaultVisibility={defaultVisibility} />
        ) : isRepoPath(path) ? (
          <RepositoryPage path={path} navigate={navigate} />
        ) : (
          <Explore kind={path.startsWith("/datasets") ? "dataset" : "model"} navigate={navigate} />
        )}
      </main>
    </div>
  );
}

/* ---------------------------------------------------------------- explore */

type SortKey = "updated" | "name" | "downloads";
const SORT_OPTIONS: [SortKey, string][] = [
  ["updated", "Recently updated"],
  ["name", "Name (A–Z)"],
  ["downloads", "Most downloads"],
];

function Explore({ kind, navigate }: { kind: "model" | "dataset"; navigate: (p: string) => void }) {
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

/* ---------------------------------------------------------- new repository */

function NewRepository({ kind, user, navigate, defaultVisibility }: { kind: "model" | "dataset"; user: User; navigate: (p: string) => void; defaultVisibility: "public" | "private" }) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState(defaultVisibility);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      setError("");
      setBusy(true);
      await api("/repositories", { method: "POST", body: JSON.stringify({ kind, name, description, visibility }) });
      navigate(kind === "dataset" ? `/datasets/${user.username}/${name}` : `/${user.username}/${name}`);
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  return (
    <section className="narrow-page enter">
      <p className="eyebrow">New / {kind}</p>
      <h1>Create a {kind} repository</h1>
      <p>Start with an empty home, then upload files from the OpenHug CLI.</p>
      <form className="card form-card" onSubmit={submit}>
        <Field label="Repository name">
          <div className="compound">
            <span>{user.username} /</span>
            <Input autoFocus placeholder={`my-${kind}`} value={name} onChange={(e) => setName(e.target.value)} />
          </div>
        </Field>
        <Field label="Description" hint="Optional — shown alongside the name in the registry.">
          <Input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="A short description of this repository" />
        </Field>
        <Field label="Visibility">
          <div className="radio-cards">
            <button type="button" className={`radio-card ${visibility === "public" ? "selected" : ""}`} onClick={() => setVisibility("public")}>
              <Globe />
              <span><strong>Public</strong><small>Anyone on this instance can download.</small></span>
            </button>
            <button type="button" className={`radio-card ${visibility === "private" ? "selected" : ""}`} onClick={() => setVisibility("private")}>
              <Lock />
              <span><strong>Private</strong><small>Only you can access it.</small></span>
            </button>
          </div>
        </Field>
        {error && <Alert kind="error">{error}</Alert>}
        <div className="form-actions">
          <Button variant="ghost" type="button" onClick={() => history.back()}>Cancel</Button>
          <Button type="submit" disabled={!name.trim() || busy}>
            {busy ? "Creating…" : "Create repository"}<ArrowRight />
          </Button>
        </div>
      </form>
    </section>
  );
}

/* -------------------------------------------------------- repository page */

function PublicRepositoryPage({ instance, path, navigate, onSignIn }: { instance: string; path: string; navigate: (p: string) => void; onSignIn: () => void }) {
  return (
    <main>
      <header className="topbar">
        <Brand name={instance} onNavigate={() => navigate("/models")} />
        <Button size="sm" variant="outline" onClick={onSignIn}>Sign in</Button>
      </header>
      <div className="content public-content">
        <RepositoryPage path={path} navigate={navigate} />
      </div>
    </main>
  );
}

function RepositoryPage({ path, navigate }: { path: string; navigate: (p: string) => void }) {
  const parts = path.split("/").filter(Boolean);
  const dataset = parts[0] === "datasets";
  const owner = dataset ? parts[1] : parts[0];
  const name = dataset ? parts[2] : parts[1];
  const kind = dataset ? "dataset" : "model";

  const [repo, setRepo] = useState<Repo | null>(null);
  const [failed, setFailed] = useState(false);
  const [tab, setTab] = useState<"files" | "history">("files");
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [copied, copy] = useCopied();

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

  return (
    <section className="enter">
      <div className="breadcrumbs">
        {dataset ? <Database /> : <Box />}
        {dataset ? "Datasets" : "Models"}
        <span className="sep">/</span>
        {owner}
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
        <Button size="sm" variant="outline" onClick={() => copy(uploadCommand)}>
          {copied ? <Check /> : <Copy />}{copied ? "Copied" : "Copy upload command"}
        </Button>
      </div>

      {tab === "files" ? (
        <div className="card file-table">
          {repo.files && repo.files.length > 0 && (
            <div className="file-header"><span>Name</span><span>Size</span><span>Digest</span><span /></div>
          )}
          {repo.files?.map((file) => (
            <div className="file-row" key={file.path}>
              <span className="file-name"><File /><span>{file.path}</span></span>
              <span className="file-size">{formatBytes(file.size)}</span>
              <code className="hash-chip">{file.sha256.slice(0, 10)}</code>
              <a className="icon-btn" title={`Download ${file.path}`} aria-label={`Download ${file.path}`}
                href={`/api/v1/repositories/${kind}/${owner}/${name}/resolve/main/${file.path}`}>
                <Download />
              </a>
            </div>
          ))}
          {(!repo.files || repo.files.length === 0) && (
            <div className="empty">
              <span className="empty-icon"><FolderGit2 /></span>
              <strong>This repository is empty</strong>
              <p>Push the first commit from your machine with the OpenHug CLI:</p>
              <code className="snippet">{uploadCommand}</code>
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

/* --------------------------------------------------------------- settings */

function SettingsPage({ user, onThemeChange, logout }: { user: User; onThemeChange: (theme: ThemeMode) => Promise<void>; logout: () => void }) {
  const [tab, setTab] = useState<"general" | "tokens" | "admin">("general");
  return (
    <section className="settings-page enter">
      <nav className="settings-nav">
        <p className="nav-label">Settings</p>
        <button className={tab === "general" ? "active" : ""} onClick={() => setTab("general")}>
          <Settings />General
        </button>
        <button className={tab === "tokens" ? "active" : ""} onClick={() => setTab("tokens")}>
          <KeyRound />API tokens
        </button>
        {user.role === "superuser" && (
          <button className={tab === "admin" ? "active" : ""} onClick={() => setTab("admin")}>
            <ShieldCheck />Administration
          </button>
        )}
        <div className="nav-sep" />
        <button className="sign-out" onClick={logout}>
          <LogOut />Sign out
        </button>
      </nav>
      {tab === "general" ? <GeneralPanel user={user} onThemeChange={onThemeChange} /> : tab === "admin" ? <AdminPanel /> : <TokenPanel />}
    </section>
  );
}

function GeneralPanel({ user, onThemeChange }: { user: User; onThemeChange: (theme: ThemeMode) => Promise<void> }) {
  const [busy, setBusy] = useState<ThemeMode | null>(null);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");

  const saveTheme = async (theme: ThemeMode) => {
    if (theme === user.theme) return;
    try {
      setError("");
      setBusy(theme);
      await onThemeChange(theme);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="settings-content">
      <div className="page-head">
        <div>
          <p className="eyebrow">Settings</p>
          <h1>General</h1>
          <p className="sub">Choose how OpenHug appears when you sign in with this account.</p>
        </div>
      </div>

      <div className="card settings-card">
        <div className="card-head">
          <h2>Appearance</h2>
          <p>Your preference is stored in the database and follows your account.</p>
        </div>
        <div className="card-body">
          <div className="theme-options" role="radiogroup" aria-label="Theme preference">
            <button
              type="button"
              role="radio"
              aria-checked={user.theme === "light"}
              className={`theme-choice ${user.theme === "light" ? "selected" : ""}`}
              onClick={() => saveTheme("light")}
              disabled={busy !== null}
            >
              <span className="theme-preview theme-preview-light"><Sun /></span>
              <span><strong>Light</strong><small>Warm paper and high-contrast ink.</small></span>
              {busy === "light" ? <span className="mini-loader" /> : user.theme === "light" && <Check />}
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={user.theme === "dark"}
              className={`theme-choice ${user.theme === "dark" ? "selected" : ""}`}
              onClick={() => saveTheme("dark")}
              disabled={busy !== null}
            >
              <span className="theme-preview theme-preview-dark"><Moon /></span>
              <span><strong>Dark</strong><small>Low-glow surfaces for late sessions.</small></span>
              {busy === "dark" ? <span className="mini-loader" /> : user.theme === "dark" && <Check />}
            </button>
          </div>
          {error && <Alert kind="error">{error}</Alert>}
        </div>
        <div className="card-foot">
          {saved && <span className="saved-note"><Check />Saved</span>}
        </div>
      </div>
    </div>
  );
}

function TokenPanel() {
  const [tokens, setTokens] = useState<Token[] | null>(null);
  const [created, setCreated] = useState("");
  const [name, setName] = useState("CLI token");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [copied, copy] = useCopied();

  const load = () => api<Token[]>("/tokens").then(setTokens).catch(() => setTokens([]));
  useEffect(() => { load(); }, []);

  const create = async () => {
    try {
      setError("");
      setBusy(true);
      const value = await api<{ token: string }>("/tokens", {
        method: "POST",
        body: JSON.stringify({ name: name.trim() || "CLI token", scopes: ["read", "write"] }),
      });
      setCreated(value.token);
      load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (id: string) => {
    try {
      await api(`/tokens/${id}`, { method: "DELETE" });
      setConfirmId(null);
      load();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="settings-content">
      <div className="page-head">
        <div>
          <p className="eyebrow">Settings</p>
          <h1>API tokens</h1>
          <p className="sub">Tokens authenticate the CLI and programmatic uploads.</p>
        </div>
      </div>

      <div className="card settings-card">
        <div className="card-head">
          <h2>Create a token</h2>
          <p>A token's value is shown exactly once, right after creation.</p>
        </div>
        <div className="card-body">
          <div className="token-create-row">
            <Input aria-label="Token name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Token name, e.g. Laptop CLI" />
            <Button onClick={create} disabled={busy}><Plus />{busy ? "Creating…" : "Create token"}</Button>
          </div>
          {error && <Alert kind="error">{error}</Alert>}
        </div>
        {created && (
          <div className="token-secret">
            <div>
              <strong><Check />Copy your new token now — it won't be shown again</strong>
              <code>{created}</code>
            </div>
            <button className="icon-btn" onClick={() => copy(created)} aria-label="Copy token">
              {copied ? <Check /> : <Copy />}
            </button>
            <button className="icon-btn" onClick={() => setCreated("")} aria-label="Dismiss"><X /></button>
          </div>
        )}
      </div>

      <div className="card settings-card">
        <div className="card-head">
          <h2>Your tokens</h2>
          <p>{tokens ? `${tokens.length} active` : "Loading…"}</p>
        </div>
        {tokens && tokens.length > 0 ? (
          <div>
            {tokens.map((token) => (
              <div className="token-row" key={token.id}>
                <span className="token-icon"><KeyRound /></span>
                <span className="token-main">
                  <strong>{token.name}</strong>
                  <small>
                    Created {formatDate(token.created_at)} · {token.last_used_at ? `Last used ${timeAgo(token.last_used_at)}` : "Never used"}
                  </small>
                </span>
                <span className="token-actions">
                  <code className="hash-chip">{token.id.slice(0, 8)}</code>
                  {confirmId === token.id ? (
                    <span className="revoke-confirm">
                      Revoke?
                      <Button size="sm" variant="danger" onClick={() => revoke(token.id)}>Confirm</Button>
                      <Button size="sm" variant="ghost" onClick={() => setConfirmId(null)}>Cancel</Button>
                    </span>
                  ) : (
                    <button className="icon-btn" title="Revoke token" aria-label={`Revoke ${token.name}`} onClick={() => setConfirmId(token.id)}>
                      <Trash2 />
                    </button>
                  )}
                </span>
              </div>
            ))}
          </div>
        ) : (
          tokens && (
            <div className="empty">
              <span className="empty-icon"><KeyRound /></span>
              <strong>No API tokens</strong>
              <p>Create one above to use the OpenHug CLI.</p>
            </div>
          )
        )}
      </div>
    </div>
  );
}

function AdminPanel() {
  const [settings, setSettings] = useState<AdminSettings | null>(null);
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const load = () =>
    Promise.all([
      api<AdminSettings>("/admin/settings").then(setSettings),
      api<AdminUser[]>("/admin/users").then(setUsers),
    ]);
  useEffect(() => { load(); }, []);

  const save = async () => {
    if (!settings) return;
    try {
      setError("");
      setBusy(true);
      setSettings(await api<AdminSettings>("/admin/settings", { method: "PUT", body: JSON.stringify(settings) }));
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const approve = async (user: AdminUser) => {
    await api(`/admin/users/${user.id}`, { method: "PATCH", body: JSON.stringify({ status: "active" }) });
    load();
  };

  if (!settings) return <div className="page-loading"><div className="loader" /></div>;

  return (
    <div className="settings-content">
      <div className="page-head">
        <div>
          <p className="eyebrow">Settings</p>
          <h1>Administration</h1>
          <p className="sub">
            Application policy lives here. Infrastructure connections remain environment-only
            and cannot be viewed or changed in the browser.
          </p>
        </div>
      </div>

      <div className="card settings-card">
        <div className="card-head">
          <h2>Instance</h2>
          <p>Naming, access policy, and defaults for new repositories.</p>
        </div>
        <div className="card-body">
          <Field label="Instance name">
            <Input value={settings.instance_name} onChange={(e) => setSettings({ ...settings, instance_name: e.target.value })} />
          </Field>
          <Field label="Signup policy" hint="Controls what happens when someone creates an account.">
            <Select value={settings.signup_policy} onChange={(v) => setSettings({ ...settings, signup_policy: v })}
              options={[["approval", "Require admin approval"], ["immediate", "Immediate access"], ["disabled", "Disable signups"]]} />
          </Field>
          <Field label="Default visibility" hint="Applied to newly created repositories.">
            <Select value={settings.default_visibility} onChange={(v) => setSettings({ ...settings, default_visibility: v })}
              options={[["public", "Public"], ["private", "Private"]]} />
          </Field>
          <Field label="Retention period" hint="Days to retain deleted repositories before cleanup.">
            <Input type="number" min={1} max={3650} value={settings.retention_days} onChange={(e) => setSettings({ ...settings, retention_days: Number(e.target.value) })} />
          </Field>
          {error && <Alert kind="error">{error}</Alert>}
        </div>
        <div className="card-foot">
          {saved && <span className="saved-note"><Check />Saved</span>}
          <Button onClick={save} disabled={busy}>{busy ? "Saving…" : "Save policy"}</Button>
        </div>
      </div>

      <div className="card settings-card">
        <div className="card-head">
          <h2>Users</h2>
          <p>{users.length} registered · {users.filter((u) => u.status === "pending").length} awaiting approval</p>
        </div>
        <div>
          {users.map((u) => (
            <div className="user-row" key={u.id}>
              <span className="avatar">{u.username.slice(0, 2).toUpperCase()}</span>
              <span className="user-main">
                <strong>
                  {u.username}
                  {u.role === "superuser" && <span className="role-chip">Admin</span>}
                </strong>
                <small>{u.email}</small>
              </span>
              <span className={`pill pill-${u.status}`}>{u.status}</span>
              {u.status === "pending"
                ? <Button size="sm" variant="outline" onClick={() => approve(u)}>Approve</Button>
                : <span />}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------- shared pieces */

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small>{hint}</small>}
    </label>
  );
}

function Select({ value, onChange, options }: { value: string; onChange: (v: string) => void; options: string[][] }) {
  return (
    <div className="select">
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
      </select>
      <ChevronDown />
    </div>
  );
}

function Alert({ kind, children }: { kind: "error" | "notice"; children: ReactNode }) {
  return (
    <p className={kind} role={kind === "error" ? "alert" : "status"}>
      {kind === "error" ? <CircleAlert /> : <Check />}
      <span>{children}</span>
    </p>
  );
}

function Popover({ trigger, children }: {
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="popover" ref={ref}>
      {trigger({ open, toggle: () => setOpen((o) => !o) })}
      {open && <div className="menu">{children(() => setOpen(false))}</div>}
    </div>
  );
}

function isRepoPath(path: string) {
  const p = path.split("/").filter(Boolean);
  return p[0] === "datasets" ? p.length === 3 : p.length === 2 && !["new", "settings", "models"].includes(p[0]);
}
