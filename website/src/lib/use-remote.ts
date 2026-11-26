"use client";

import { useCallback, useEffect, useRef, useState } from "react";

export type RemoteState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

/**
 * One remote read with the three states every Games view needs. A newer request supersedes an older one, and an
 * unmounted view never updates. `reload` repeats the read; it shows the loading state again rather than stale data.
 */
export function useRemote<T>(loader: (signal: AbortSignal) => Promise<T>, deps: ReadonlyArray<unknown>, enabled = true) {
  const [state, setState] = useState<RemoteState<T>>({ status: "loading" });
  const [nonce, setNonce] = useState(0);
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    setState({ status: "loading" });
    loaderRef
      .current(controller.signal)
      .then((data) => {
        if (!controller.signal.aborted) setState({ status: "ready", data });
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setState({ status: "error", message: error instanceof Error ? error.message : "Something went wrong." });
      });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce, enabled]);

  const reload = useCallback(() => setNonce((value) => value + 1), []);
  return { state, reload };
}
