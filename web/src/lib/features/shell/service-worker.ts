import { dev } from "$app/environment";

export function registerServiceWorker(): () => void {
  if (dev || typeof navigator === "undefined" || !("serviceWorker" in navigator)) {
    return () => {};
  }

  const hadController = Boolean(navigator.serviceWorker.controller);

  function onControllerChange() {
    if (hadController) {
      window.location.reload();
    }
  }

  navigator.serviceWorker.addEventListener("controllerchange", onControllerChange, { once: true });

  let storedRegistration: ServiceWorkerRegistration | undefined;

  function onUpdateFound() {
    const installing = storedRegistration?.installing;
    if (!installing) {
      return;
    }
    installing.addEventListener("statechange", () => {
      if (installing.state === "installed" && navigator.serviceWorker.controller) {
        installing.postMessage({ type: "SKIP_WAITING" });
      }
    });
  }

  navigator.serviceWorker
    .register("/sw.js")
    .then((registration) => {
      storedRegistration = registration;
      registration.addEventListener("updatefound", onUpdateFound);
    })
    .catch((err) => {
      console.error("Service worker registration failed", err);
    });

  return () => {
    navigator.serviceWorker.removeEventListener("controllerchange", onControllerChange);
    storedRegistration?.removeEventListener("updatefound", onUpdateFound);
  };
}
