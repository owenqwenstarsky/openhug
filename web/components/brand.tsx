export function Brand({ name = "OpenHug", onNavigate }: { name?: string; onNavigate?: () => void }) {
  if (onNavigate)
    return (
      <button className="brand" onClick={onNavigate} aria-label={`${name} home`}>
        <strong>{name}</strong>
      </button>
    );
  return (
    <span className="brand">
      <strong>{name}</strong>
    </span>
  );
}

export function Splash() {
  return (
    <main className="splash">
      <Brand />
      <div className="loader" />
    </main>
  );
}
