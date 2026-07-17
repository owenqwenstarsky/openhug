"use client";

import { useEffect } from "react";
import {
  Box, ChevronDown, Database, LogOut, Moon, Plus, Settings, Sun,
} from "lucide-react";
import { Brand } from "@/components/brand";
import { Explore } from "@/components/explore";
import { NewRepository } from "@/components/new-repository";
import { RepoWorkspace } from "@/components/repo/workspace";
import { SettingsPage } from "@/components/settings/settings-page";
import { Button } from "@/components/ui/button";
import { Popover } from "@/components/ui/popover";
import { parseRepoRoute } from "@/lib/repo-routing";
import type { ThemeMode, User } from "@/lib/types";

export function Shell({ instance, user, path, navigate, defaultVisibility, onThemeChange, logout }: {
  instance: string; user: User; path: string; navigate: (p: string) => void; defaultVisibility: "public" | "private"; onThemeChange: (theme: ThemeMode) => Promise<void>; logout: () => void;
}) {
  const link = (to: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    navigate(to);
  };

  useEffect(() => {
    const parts = path.split("/").filter(Boolean);
    const route = parseRepoRoute(path);
    let label = "Models";
    if (path === "/settings") label = "Settings";
    else if (path.startsWith("/new/")) label = "New repository";
    else if (route?.page === "blob" || route?.page === "edit") label = route.filePath;
    else if (route?.page === "new-file") label = `New file · ${route.name}`;
    else if (route?.page === "repo") label = `${route.owner}/${route.name}`;
    else if (path.startsWith("/datasets")) label = parts.length >= 3 ? parts.slice(1, 3).join("/") : "Datasets";
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
        ) : parseRepoRoute(path) ? (
          <RepoWorkspace path={path} navigate={navigate} user={user} />
        ) : (
          <Explore kind={path.startsWith("/datasets") ? "dataset" : "model"} navigate={navigate} />
        )}
      </main>
    </div>
  );
}
