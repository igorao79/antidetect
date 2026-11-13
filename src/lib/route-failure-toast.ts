import type { TFunction } from "i18next";
import {
  type DiagnoseOptions,
  diagnoseRouteFailure,
  diagnosisSentence,
} from "@/lib/proxy-check-diagnosis";
import { showErrorToast } from "@/lib/toast-utils";

/** How long the opened explanation stays, longer than a plain error. */
const DETAILS_TOAST_MS = 20000;

interface RouteFailureToast extends DiagnoseOptions {
  title: string;
  /** The raw error, which the diagnosis reads. */
  error: unknown;
  id?: string;
}

/**
 * An error toast that, when the failure is about the route, offers
 * "Details": the same toast then opens into what it means and what to try.
 * Anything else is the plain error toast it always was.
 */
export function showRouteFailureToast(
  t: TFunction,
  { title, error, id, assumeRoute }: RouteFailureToast,
) {
  const diagnosis = diagnoseRouteFailure(error, { assumeRoute });
  if (!diagnosis) return showErrorToast(title, { id });
  const toastId = id ?? `route-failure-${Date.now()}`;
  return showErrorToast(title, {
    id: toastId,
    action: {
      label: t("routeDiagnosis.details"),
      onClick: () => {
        showErrorToast(title, {
          id: toastId,
          sound: null,
          duration: DETAILS_TOAST_MS,
          description: diagnosisSentence(t, diagnosis),
        });
      },
    },
  });
}
