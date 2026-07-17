"use client";

import { useState } from "react";
import { KeyRound, LogOut, Settings, ShieldCheck } from "lucide-react";
import { AdminPanel } from "@/components/settings/admin-panel";
import { GeneralPanel } from "@/components/settings/general-panel";
import { TokenPanel } from "@/components/settings/token-panel";
import type { ThemeMode, User } from "@/lib/types";

export function SettingsPage({ user, onThemeChange, logout }: { user: User; onThemeChange: (theme: ThemeMode) => Promise<void>; logout: () => void }) {
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
