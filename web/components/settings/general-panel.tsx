"use client";

import { useState } from "react";
import { Check, Moon, Sun } from "lucide-react";
import { Alert } from "@/components/ui/alert";
import type { ThemeMode, User } from "@/lib/types";

export function GeneralPanel({ user, onThemeChange }: { user: User; onThemeChange: (theme: ThemeMode) => Promise<void> }) {
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
