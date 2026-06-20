import { client } from "$lib/api/client";
import type { components } from "$lib/api/schema";

type GetFxRatesResponse = components["schemas"]["GetFxRatesResponse"];
type ExchangeRateRow = components["schemas"]["ExchangeRateRow"];

export function getFxRates(params: {
  from: string;
  to: string;
  quotes: string[];
}): Promise<ExchangeRateRow[]> {
  return client
    .get<GetFxRatesResponse>("/fx/rates", {
      from: params.from,
      to: params.to,
      quotes: params.quotes,
    })
    .then((r) => r.rates);
}
