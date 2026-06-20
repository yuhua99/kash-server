import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type Category = components["schemas"]["Category"];
type GetCategoriesResponse = components["schemas"]["GetCategoriesResponse"];

export function getCategories(params: {
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<GetCategoriesResponse> {
  return client.get<GetCategoriesResponse>("/categories", {
    search: params.search,
    limit: params.limit,
    offset: params.offset,
  });
}

export function createCategory(body: { name: string; is_income: boolean }): Promise<Category> {
  return client.post<Category>("/categories", body);
}

export function updateCategory(id: string, body: { name: string }): Promise<Category> {
  return client.put<Category>(`/categories/${id}`, body);
}

export function deleteCategory(id: string): Promise<void> {
  return client.del<void>(`/categories/${id}`);
}
