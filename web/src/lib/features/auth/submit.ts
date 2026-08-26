import { getErrorMessage } from "$lib/api/errors";
import { toast } from "$lib/ui/toast";
import { validatePassword, validateUsername } from "$lib/validation";

type AuthSubmitOptions = {
  event: SubmitEvent;
  username: string;
  password: string;
  onValidSubmit: (username: string, password: string) => Promise<void>;
  setUsernameError: (message: string | null) => void;
  setPasswordError: (message: string | null) => void;
  setPending: (pending: boolean) => void;
  fallbackErrorMessage: string;
};

export async function handleAuthSubmit(opts: AuthSubmitOptions): Promise<void> {
  opts.event.preventDefault();

  const u = opts.username.trim();
  const p = opts.password.trim();
  const ue = validateUsername(u);
  const pe = validatePassword(p);

  opts.setUsernameError(ue);
  opts.setPasswordError(pe);

  if (ue || pe) {
    return;
  }

  opts.setPending(true);

  try {
    await opts.onValidSubmit(u, p);
  } catch (error) {
    toast.error(getErrorMessage(error, opts.fallbackErrorMessage));
  } finally {
    opts.setPending(false);
  }
}
