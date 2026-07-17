"use client";

import { ReactNode } from "react";
import { Box, Database } from "lucide-react";

export function FilePageShell({
  route, filePath, navigate, children, actions,
}: {
  route: { kind: "model" | "dataset"; owner: string; name: string; base: string };
  filePath?: string;
  navigate: (p: string) => void;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const dataset = route.kind === "dataset";
  return (
    <section className="enter file-page">
      <div className="breadcrumbs">
        <button type="button" className="crumb-link" onClick={() => navigate(dataset ? "/datasets" : "/models")}>
          {dataset ? <Database /> : <Box />}
          {dataset ? "Datasets" : "Models"}
        </button>
        <span className="sep">/</span>
        <button type="button" className="crumb-link" onClick={() => navigate(route.base)}>
          {route.owner}
        </button>
        <span className="sep">/</span>
        <button type="button" className="crumb-link" onClick={() => navigate(route.base)}>
          {route.name}
        </button>
        {filePath && (
          <>
            <span className="sep">/</span>
            <span className="crumb-current">{filePath}</span>
          </>
        )}
      </div>
      <div className="file-page-head">
        <div>
          <h1>{filePath || "New file"}</h1>
        </div>
        {actions && <div className="file-page-actions">{actions}</div>}
      </div>
      {children}
    </section>
  );
}
