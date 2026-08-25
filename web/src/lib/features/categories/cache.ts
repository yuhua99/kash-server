import { createCache } from "$lib/cache";
import { getCategories } from "$lib/features/categories/api";

const categoriesCache = createCache(() =>
  getCategories({ limit: 1000, offset: 0 }).then((r) => r.categories),
);

export const getCategoriesCached = () => categoriesCache.get();
export const invalidateCategoriesCache = () => categoriesCache.invalidate();
