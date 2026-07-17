"use client";

import { CircleAlert } from "lucide-react";
import { FileCreatePage } from "@/components/files/file-create-page";
import { FileEditPage } from "@/components/files/file-edit-page";
import { FileViewPage } from "@/components/files/file-view-page";
import { RepositoryPage } from "@/components/repo/repository-page";
import { Button } from "@/components/ui/button";
import { parseRepoRoute } from "@/lib/repo-routing";
import type { User } from "@/lib/types";

export function RepoWorkspace({ path, navigate, user }: { path: string; navigate: (p: string) => void; user: User | null }) {
  const route = parseRepoRoute(path);
  if (!route) {
    return (
      <div className="empty empty-dashed enter">
        <span className="empty-icon"><CircleAlert /></span>
        <strong>Page not found</strong>
        <Button size="sm" variant="outline" onClick={() => navigate("/models")}>Back to models</Button>
      </div>
    );
  }
  if (route.page === "blob") {
    return <FileViewPage route={route} navigate={navigate} user={user} />;
  }
  if (route.page === "edit") {
    return <FileEditPage route={route} navigate={navigate} user={user} />;
  }
  if (route.page === "new-file") {
    return <FileCreatePage route={route} navigate={navigate} user={user} />;
  }
  return <RepositoryPage route={route} navigate={navigate} user={user} />;
}
