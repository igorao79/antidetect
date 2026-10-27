import { JetBrains_Mono, Onest } from "next/font/google";
import "@/styles/globals.css";
import "flag-icons/css/flag-icons.min.css";
import { ClientProviders } from "@/components/client-providers";

// The interface is used in Russian as much as in English, so both faces are
// loaded with their Cyrillic: a Latin-only face leaves Cyrillic to the system
// font and the two never match.
const appSans = Onest({
  variable: "--font-app-sans",
  subsets: ["latin", "cyrillic"],
});

const appMono = JetBrains_Mono({
  variable: "--font-app-mono",
  subsets: ["latin", "cyrillic"],
});

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${appSans.variable} ${appMono.variable}`}
      suppressHydrationWarning
    >
      <body className="antialiased overflow-hidden bg-background">
        <ClientProviders>{children}</ClientProviders>
      </body>
    </html>
  );
}
