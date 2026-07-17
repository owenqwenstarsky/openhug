"use client";

import { useEffect, useState } from "react";
import { Check, Copy, KeyRound, Plus, Trash2, X } from "lucide-react";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useCopied } from "@/hooks/use-copied";
import { api } from "@/lib/api";
import { formatDate, timeAgo } from "@/lib/format";
import type { Token } from "@/lib/types";

export function TokenPanel() {
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
