import type { ReactNode } from "react";

export function Card({
  title,
  children,
  className = "",
}: {
  title: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`card ${className}`}>
      <h3>{title}</h3>
      {children}
    </section>
  );
}
