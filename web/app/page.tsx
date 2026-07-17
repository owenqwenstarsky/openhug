"use client";

import { useEffect, useState } from "react";
import { Auth } from "@/components/auth";
import { Splash } from "@/components/brand";
import { Onboarding } from "@/components/onboarding";
import { PublicRepositoryPage } from "@/components/repo/public-page";
import { Shell } from "@/components/shell";
import { api } from "@/lib/api";
import { normalizeTheme } from "@/lib/authz";
import { parseRepoRoute } from "@/lib/repo-routing";
import type { ThemeMode, User } from "@/lib/types";

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
  if (!user && parseRepoRoute(path))
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
