"use client";

import { useTranslation } from "react-i18next";
import {
  ROUTE_LOCAL_NOTE_KEY,
  type RouteDiagnosis,
} from "@/lib/proxy-check-diagnosis";

/** What a route failure means and what to try, under the failure itself. */
export function RouteDiagnosisNote({
  diagnosis,
}: {
  diagnosis: RouteDiagnosis;
}) {
  const { t } = useTranslation();
  return (
    <div
      data-slot="route-diagnosis"
      data-kind={diagnosis.kind}
      className="space-y-1 rounded-md bg-foreground/5 p-2.5 text-xs leading-relaxed"
    >
      <p className="text-foreground">{t(diagnosis.meaningKey)}</p>
      <p className="text-muted-foreground">
        <span className="font-medium text-foreground">
          {t("routeDiagnosis.tryLabel")}
        </span>{" "}
        {t(diagnosis.actionKey)}
      </p>
      {diagnosis.localNetworkMatters && (
        <p className="text-muted-foreground">{t(ROUTE_LOCAL_NOTE_KEY)}</p>
      )}
    </div>
  );
}
