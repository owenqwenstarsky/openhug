"use client";

import { useState } from "react";
import { ArrowRight, Check, Database } from "lucide-react";
import { Brand } from "@/components/brand";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import type { User } from "@/lib/types";

export function Onboarding({ setupTokenRequired, onDone }: { setupTokenRequired: boolean; onDone: (name: string, user: User) => void }) {
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
