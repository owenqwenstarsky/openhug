"use client";

import { FormEvent, useState } from "react";
import { ArrowRight, Globe, Lock } from "lucide-react";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { User } from "@/lib/types";

export function NewRepository({ kind, user, navigate, defaultVisibility }: { kind: "model" | "dataset"; user: User; navigate: (p: string) => void; defaultVisibility: "public" | "private" }) {
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
