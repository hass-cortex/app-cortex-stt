import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { Spinner } from "./spinner";

type ButtonVariant = "primary" | "secondary" | "outline" | "ghost" | "danger";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
	ref?: Ref<HTMLButtonElement>;
	variant?: ButtonVariant;
	size?: ButtonSize;
	loading?: boolean;
	icon?: ReactNode;
	children?: ReactNode;
}

const variantClasses: Record<ButtonVariant, string> = {
	primary:
		"bg-accent text-white border border-accent hover:bg-accent-hover hover:border-accent-hover",
	secondary: "bg-surface-3 text-text-primary border border-border hover:border-text-muted",
	outline:
		"bg-transparent text-text-secondary border border-border hover:text-text-primary hover:border-text-muted",
	ghost:
		"bg-transparent text-text-secondary border border-transparent hover:bg-surface-3 hover:text-text-primary",
	danger: "bg-error-wash text-error border border-transparent hover:border-error/40",
};

const sizeClasses: Record<ButtonSize, string> = {
	sm: "px-2.5 py-1 text-xs gap-1.5",
	md: "px-3 py-1.5 text-[12.5px] gap-[7px]",
	lg: "px-5 py-2.5 text-sm gap-2",
};

export function Button({
	ref,
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
			ref={ref}
			className={`inline-flex items-center justify-center rounded-[7px] font-medium whitespace-nowrap transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
			disabled={disabled || loading}
			{...props}
		>
			{loading ? <Spinner size="sm" /> : icon}
			{children}
		</button>
	);
}
