import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

const win = getCurrentWindow();

const button =
  "ui flex h-11 w-[46px] items-center justify-center text-muted hover:bg-white/6 hover:text-body";

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let alive = true;
    const read = () => {
      void win.isMaximized().then((value) => {
        if (alive) setMaximized(value);
      });
    };
    read();
    const unlisten = win.onResized(read);
    return () => {
      alive = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div className="flex h-full items-stretch">
      <button
        type="button"
        aria-label="Minimise"
        className={button}
        onClick={() => void win.minimize()}
      >
        <Minus size={14} strokeWidth={1.6} />
      </button>
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximise"}
        className={button}
        onClick={() => void win.toggleMaximize()}
      >
        {maximized ? <Copy size={12} strokeWidth={1.6} /> : <Square size={12} strokeWidth={1.6} />}
      </button>
      <button
        type="button"
        aria-label="Close"
        className="ui flex h-11 w-[46px] items-center justify-center text-muted hover:bg-danger-fill hover:text-white"
        onClick={() => void win.close()}
      >
        <X size={14} strokeWidth={1.6} />
      </button>
    </div>
  );
}
