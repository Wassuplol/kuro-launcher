export function greeting(now: Date = new Date()): string {
  const hour = now.getHours();
  if (hour < 5) return "Still up";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/** "Horror, RPG and 2 more" without pretending to know what the list means. */
export function listNames(names: string[]): string {
  if (names.length === 0) return "";
  if (names.length === 1) return names[0] ?? "";
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  const rest = names.length - 2;
  return `${names[0]}, ${names[1]} and ${rest} more`;
}

/** Bytes as a person reads them. One decimal only where it changes the number. */
export function bytes(value: number): string {
  if (value <= 0) return "0 MB";
  const mb = value / 1_000_000;
  if (mb < 1) return `${Math.max(1, Math.round(value / 1000))} KB`;
  if (mb < 1000) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  const gb = value / 1_000_000_000;
  return `${gb.toFixed(2)} GB`;
}

/** The clock time of a moment given in unix seconds. */
export function clock(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A short date, for "last touched" style metadata. */
export function day(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
  });
}

/** "6m 41s", "1h 12m", "48s". Rounded, never two units of noise. */
export function span(seconds: number): string {
  if (seconds >= 3600) {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
  }
  if (seconds >= 60) return `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)}s`;
  return `${Math.max(0, Math.round(seconds))}s`;
}

/** "18 Nov, 16:13": the day matters more than the year on a list of launches. */
export function stamp(unix: number): string {
  return `${day(unix)}, ${clock(unix)}`;
}

/** "3 minutes ago", for things whose exact timestamp nobody reads. */
export function ago(unix: number, now: number = Date.now()): string {
  if (!unix) return "never";
  const seconds = Math.max(0, Math.round(now / 1000 - unix));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}
