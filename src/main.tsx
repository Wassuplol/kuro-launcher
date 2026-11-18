import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { Strip } from "./screens/Strip";
import "./theme/index.css";

const client = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 30_000 },
  },
});

const host = document.getElementById("root");
if (!host) throw new Error("The page has no root element.");

/**
 * The strip is the same bundle in a second window. It is decided by the window's own
 * label rather than by the address, so a refresh or a changed hash cannot turn the
 * strip into a second copy of the whole launcher.
 */
const strip =
  getCurrentWindow().label === "strip" || window.location.hash.startsWith("#/strip");

createRoot(host).render(
  <StrictMode>
    <QueryClientProvider client={client}>{strip ? <Strip /> : <App />}</QueryClientProvider>
  </StrictMode>,
);
