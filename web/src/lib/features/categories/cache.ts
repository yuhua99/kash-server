import { createCache } from "$lib/cache";
import type { components } from "$lib/api/schema";
import { getCategories } from "$lib/features/categories/api";

type Category = components["schemas"]["Category"];

const categoriesCache = createCache(() =>
  getCategories({ limit: 1000, offset: 0 }).then((r) => r.categories),
);

export const getCategoriesCached = () => categoriesCache.get();
export const invalidateCategoriesCache = () => categoriesCache.invalidate();
export const setCategoriesCache = (list: Category[]) => categoriesCache.set(list);
