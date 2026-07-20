import type { Metadata } from "next";
import "@fontsource-variable/fredoka";
import "@fontsource-variable/nunito-sans";
import "./globals.css";

export const metadata: Metadata = {
  metadataBase: new URL("https://ndraw.app"),
  title: {
    default: "NDraw — draw together",
    template: "%s · NDraw",
  },
  description:
    "A fast, delightfully chaotic drawing game for friends. Create a room and start sketching in seconds.",
  icons: {
    icon: "/favicon.svg",
    shortcut: "/favicon.svg",
  },
  applicationName: "NDraw",
  openGraph: {
    title: "NDraw — draw together",
    description: "A serious canvas for a delightfully unserious drawing game.",
    images: [{ url: "/og.png", width: 1536, height: 1024, alt: "Two playful cats peeking around the NDraw canvas" }],
  },
  twitter: {
    card: "summary_large_image",
    images: ["/og.png"],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
