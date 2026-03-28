import type { ReactNode } from "react";

type BadgeVariant = "default" | "success" | "warning" | "error" | "info" | "accent";

interface BadgeProps {
	children: ReactNode;
	variant?: BadgeVariant;
	className?: string;
}

const variantClasses: Record<BadgeVariant, string> = {
	default: "bg-surface-3 text-text-secondary",
	success: "bg-success/15 text-success",
	warning: "bg-warning/15 text-warning",
	error: "bg-error/15 text-error",
	info: "bg-info/15 text-info",
	accent: "bg-accent/15 text-accent",
};

export function Badge({ children, variant = "default", className = "" }: BadgeProps) {
	return (
		<span
			className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${variantClasses[variant]} ${className}`}
		>
			{children}
		</span>
	);
}
