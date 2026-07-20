import type { Metadata } from "next";
import { NDrawApp } from "../components/NDrawApp";

export const metadata: Metadata = {
  title: "Draw together",
};

export default function Home() {
  return <NDrawApp />;
}
