import { ReactNode } from "react";
import { Check, CircleAlert } from "lucide-react";

export function Alert({ kind, children }: { kind: "error" | "notice"; children: ReactNode }) {
  return (
    <p className={kind} role={kind === "error" ? "alert" : "status"}>
      {kind === "error" ? <CircleAlert /> : <Check />}
      <span>{children}</span>
    </p>
  );
}
