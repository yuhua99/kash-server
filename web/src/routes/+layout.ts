import { redirect } from "@sveltejs/kit";
import { getMe } from "$lib/features/auth/api";
import { getSettings } from "$lib/features/settings/api";
import type { LayoutLoad } from "./$types";

export const ssr = false;
export const prerender = false;

const PROTECTED = ["/home", "/records", "/categories", "/stats", "/settings"];
const AUTH_ROUTES = ["/login", "/register"];

export const load: LayoutLoad = async ({ url }) => {
  let user = null;
  try {
    user = await getMe();
  } catch {
    user = null;
  }

  const path = url.pathname;

  if (path === "/") {
    redirect(307, user ? "/home" : "/login");
  }

  const isProtected = PROTECTED.some((base) => path === base || path.startsWith(`${base}/`));
  if (isProtected && !user) {
    redirect(307, "/login");
  }

  if (AUTH_ROUTES.includes(path) && user) {
    redirect(307, "/home");
  }

  let mainCurrency = "";
  if (user) {
    try {
      mainCurrency = (await getSettings()).main_currency;
    } catch {
      mainCurrency = "";
    }
  }

  return { user, mainCurrency };
};
