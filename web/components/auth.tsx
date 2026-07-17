"use client";

import { FormEvent, useState } from "react";
import { ArrowRight } from "lucide-react";
import { Brand } from "@/components/brand";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import type { User } from "@/lib/types";

export function Auth({ instance, signupPolicy, onLogin }: { instance: string; signupPolicy: string; onLogin: (u: User) => void }) {
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
