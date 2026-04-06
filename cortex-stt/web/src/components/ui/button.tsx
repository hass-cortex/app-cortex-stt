import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Spinner } from "./spinner";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
	variant?: ButtonVariant;
	size?: ButtonSize;
	loading?: boolean;
	icon?: ReactNode;
	children?: ReactNode;
}

const variantClasses: Record<ButtonVariant, string> = {
	primary:
		"bg-accent text-surface-0 hover:bg-accent-hover disabled:opacity-50 disabled:cursor-not-allowed",
	secondary:
		"bg-surface-3 text-text-primary hover:bg-border disabled:opacity-50 disabled:cursor-not-allowed",
	ghost: "text-text-secondary hover:bg-surface-3 hover:text-text-primary disabled:opacity-50",
	danger:
		"bg-error/15 text-error hover:bg-error/25 disabled:opacity-50 disabled:cursor-not-allowed",
};

const sizeClasses: Record<ButtonSize, string> = {
	sm: "px-2.5 py-1 text-xs rounded-md gap-1.5",
	md: "px-3.5 py-1.5 text-sm rounded-lg gap-2",
	lg: "px-5 py-2.5 text-base rounded-lg gap-2",
};

export function Button({
	variant = "primary",
	size = "md",
	loading = false,
	icon,
	children,
	disabled,
	className = "",
	...props
}: ButtonProps) {
	return (
		<button
			className={`inline-flex items-center justify-center font-medium transition-colors cursor-pointer ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
			disabled={disabled || loading}
			{...props}
		>
			{loading ? <Spinner size="sm" /> : icon}
			{children}
		</button>
	);
}
