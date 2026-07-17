"use client";

import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import type { AdminSettings, AdminUser } from "@/lib/types";

export function AdminPanel() {
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
