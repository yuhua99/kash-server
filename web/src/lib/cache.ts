export type Cache<T> = {
  get(): Promise<T>;
  set(value: T): void;
  invalidate(): void;
};

export function createCache<T>(fetcher: () => Promise<T>): Cache<T> {
  let value: T;
  let hasValue = false;
  let inFlight: Promise<T> | null = null;
  let version = 0;

  return {
    get(): Promise<T> {
      if (hasValue) {
        return Promise.resolve(value);
      }

      if (inFlight) {
        return inFlight;
      }

      const startVersion = version;
      const request = fetcher()
        .then((result) => {
          if (version === startVersion) {
            value = result;
            hasValue = true;
          }

          return result;
        })
        .finally(() => {
          if (inFlight === request) {
            inFlight = null;
          }
        });

      inFlight = request;
      return request;
    },

    set(nextValue: T): void {
      value = nextValue;
      hasValue = true;
    },

    invalidate(): void {
      version += 1;
      hasValue = false;
      inFlight = null;
    },
  };
}
