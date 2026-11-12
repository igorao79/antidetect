"use client";

import { useTranslation } from "react-i18next";
import { Toaster as Sonner, type ToasterProps } from "sonner";
import { useTheme } from "@/components/theme-provider";

const Toaster = ({ ...props }: ToasterProps) => {
  const { theme = "system" } = useTheme();
  const { t } = useTranslation();

  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="group toaster"
      customAriaLabel={t("toasts.region")}
      style={
        {
          "--normal-bg": "var(--card)",
          "--normal-text": "var(--card-foreground)",
          "--normal-border": "var(--border)",
          zIndex: 10001,
        } as React.CSSProperties
      }
      toastOptions={{
        style: {
          zIndex: 10001,
          pointerEvents: "auto",
          backdropFilter: "saturate(1.2)",
        },
      }}
      {...props}
    />
  );
};

export { Toaster };
