import { dev } from "$app/environment";

export function registerServiceWorker(): () => void {
  if (dev || typeof navigator === "undefined" || !("serviceWorker" in navigator)) {
    return () => {};
  }

  const hadController = Boolean(navigator.serviceWorker.controller);
  let reloading = false;

  function onControllerChange() {
    if (reloading) {
      return;
    }
    reloading = true;
    window.location.reload();
  }

  if (hadController) {
    navigator.serviceWorker.addEventListener("controllerchange", onControllerChange);
  }

  navigator.serviceWorker
    .register("/sw.js")
    .then((registration) => {
      registration.addEventListener("updatefound", () => {
        const installing = registration.installing;
        if (!installing) {
          return;
        }
        installing.addEventListener("statechange", () => {
          if (installing.state === "installed" && navigator.serviceWorker.controller) {
            installing.postMessage({ type: "SKIP_WAITING" });
          }
        });
      });
    })
    .catch(() => {});

  return () => {
    if (hadController) {
      navigator.serviceWorker.removeEventListener("controllerchange", onControllerChange);
    }
  };
}
