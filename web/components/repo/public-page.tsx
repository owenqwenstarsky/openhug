"use client";

import { Brand } from "@/components/brand";
import { RepoWorkspace } from "@/components/repo/workspace";
import { Button } from "@/components/ui/button";

export function PublicRepositoryPage({ instance, path, navigate, onSignIn }: { instance: string; path: string; navigate: (p: string) => void; onSignIn: () => void }) {
  return (
    <main>
      <header className="topbar">
        <Brand name={instance} onNavigate={() => navigate("/models")} />
        <Button size="sm" variant="outline" onClick={onSignIn}>Sign in</Button>
      </header>
      <div className="content public-content">
        <RepoWorkspace path={path} navigate={navigate} user={null} />
      </div>
    </main>
  );
}
