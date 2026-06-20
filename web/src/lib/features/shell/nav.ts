export type NavItem = { href: string; label: string };

export const navItems: NavItem[] = [
  { href: "/home", label: "KASH!" },
  { href: "/records", label: "Records" },
  { href: "/categories", label: "Categories" },
  { href: "/stats", label: "Stats" },
  { href: "/settings", label: "Settings" },
];

export function isActive(pathname: string, href: string): boolean {
  return pathname === href || pathname.startsWith(`${href}/`);
}
