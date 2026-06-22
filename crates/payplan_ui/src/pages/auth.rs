use leptos::prelude::*;

use crate::{app::request_query, islands::ThemeToggle};

#[component]
fn AuthLayout(title: &'static str, eyebrow: &'static str, children: Children) -> impl IntoView {
    view! {
        <main class="min-h-screen bg-paper text-ink transition-colors flex items-center justify-center py-12 px-4 sm:px-6 lg:px-8 relative overflow-hidden">
            // Decorative background grids
            <div class="absolute inset-0 bg-[linear-gradient(to_right,var(--color-border)_1px,transparent_1px),linear-gradient(to_bottom,var(--color-border)_1px,transparent_1px)] bg-[size:4rem_4rem] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_50%,#000_70%,transparent_100%)] opacity-30 pointer-events-none"></div>

            <div class="max-w-md w-full space-y-8 relative z-10">
                // Header with Brand and ThemeToggle
                <div class="flex items-center justify-between pb-4 border-b border-border">
                    <a href="/" class="flex items-center gap-2 group decoration-transparent">
                        <span class="flex items-center justify-center w-8 h-8 rounded-lg bg-gradient-to-tr from-[#196c4a] to-[#2ea46f] text-white font-extrabold text-base shadow-sm group-hover:shadow-md transition-all">
                            "P"
                        </span>
                        <span class="text-lg font-black tracking-tight text-ink">"PayPlan"</span>
                    </a>
                    <ThemeToggle/>
                </div>

                // Inner panel Card
                <article class="panel p-8 sm:p-10 flex flex-col gap-6">
                    <header>
                        <p class="eyebrow text-xs font-extrabold tracking-widest text-muted uppercase mb-1">
                            {eyebrow}
                        </p>
                        <h1 class="text-3xl font-black tracking-tight text-ink">
                            {title}
                        </h1>
                    </header>
                    {children()}
                </article>

                // Footer back link
                <div class="text-center">
                    <a href="/" class="text-sm font-semibold text-muted hover:text-ink hover:underline decoration-transparent">
                        "← Back to home"
                    </a>
                </div>
            </div>
        </main>
    }
}

#[component]
pub(crate) fn LoginPage() -> impl IntoView {
    let query = request_query();
    let error = query.error.is_some();
    let next = query.next.unwrap_or_else(|| "/dashboard".into());

    view! {
        <AuthLayout title="Sign In" eyebrow="PayPlan Administration">
            {error.then(|| view! {
                <div class="error-banner border border-red-200 bg-red-50/50 text-red-700 p-4 rounded-xl text-sm flex items-start gap-2 animate-fade-in dark:bg-red-950/20 dark:border-red-900/30 dark:text-red-400">
                    <span class="font-bold">"Error:"</span>
                    <span>"Invalid email or password."</span>
                </div>
            })}

            <form method="post" action="/login" class="flex flex-col gap-5">
                <input name="next" type="hidden" value=next/>
                <label class="flex flex-col gap-2 text-sm font-bold text-ink">
                    "Email Address"
                    <input
                        name="email"
                        type="email"
                        autocomplete="email"
                        required
                        class="w-full border border-input-border rounded-xl p-3 bg-panel-bg text-ink focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent transition-all"
                        placeholder="admin@company.com"
                    />
                </label>
                <label class="flex flex-col gap-2 text-sm font-bold text-ink">
                    <div class="flex items-center justify-between">
                        "Password"
                        <a
                            href="/forgot-password"
                            class="text-xs font-semibold text-accent hover:underline decoration-transparent"
                        >
                            "Forgot password?"
                        </a>
                    </div>
                    <input
                        name="password"
                        type="password"
                        autocomplete="current-password"
                        required
                        class="w-full border border-input-border rounded-xl p-3 bg-panel-bg text-ink focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent transition-all"
                        placeholder="••••••••"
                    />
                </label>
                <button
                    type="submit"
                    class="w-full py-3.5 px-4 bg-accent hover:bg-accent/95 text-white font-bold rounded-xl transition-all shadow-md shadow-accent/15 cursor-pointer mt-2"
                >
                    "Sign In to Console"
                </button>
            </form>
        </AuthLayout>
    }
}

#[component]
pub(crate) fn ForgotPasswordPage() -> impl IntoView {
    let query = request_query();
    let is_sent = query.status.as_deref() == Some("sent");

    view! {
        <AuthLayout title="Forgot Password" eyebrow="Access Recovery">
            {if is_sent {
                view! {
                    <div class="flex flex-col gap-6">
                        <div class="border border-[#196c4a]/20 bg-[#196c4a]/10 text-[#196c4a] p-4 rounded-xl text-sm flex items-start gap-2 dark:bg-[#196c4a]/5">
                            <span class="font-bold">"Success:"</span>
                            <span>"If that email matches an account, we have sent a password reset link."</span>
                        </div>
                        <a
                            href="/login"
                            class="w-full py-3.5 px-4 bg-accent hover:bg-accent/95 text-white font-bold rounded-xl text-center decoration-transparent transition-all shadow-md shadow-accent/15"
                        >
                            "Return to Sign In"
                        </a>
                    </div>
                }.into_any()
            } else {
                view! {
                    <form method="get" action="/forgot-password" class="flex flex-col gap-5">
                        <input type="hidden" name="status" value="sent"/>
                        <p class="text-sm text-[#527064] font-medium leading-relaxed">
                            "Enter the email associated with your administration account. We will send you instructions to safely reset your password."
                        </p>
                        <label class="flex flex-col gap-2 text-sm font-bold text-ink">
                            "Email Address"
                            <input
                                name="email"
                                type="email"
                                autocomplete="email"
                                required
                                class="w-full border border-input-border rounded-xl p-3 bg-panel-bg text-ink focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent transition-all"
                                placeholder="name@domain.com"
                            />
                        </label>
                        <button
                            type="submit"
                            class="w-full py-3.5 px-4 bg-accent hover:bg-accent/95 text-white font-bold rounded-xl transition-all shadow-md shadow-accent/15 cursor-pointer mt-2"
                        >
                            "Send Reset Instructions"
                        </button>
                        <div class="text-center mt-2">
                            <a
                                href="/login"
                                class="text-sm font-bold text-accent hover:underline decoration-transparent"
                            >
                                "Return to Sign In"
                            </a>
                        </div>
                    </form>
                }.into_any()
            }}
        </AuthLayout>
    }
}

#[component]
pub(crate) fn ResetPasswordPage() -> impl IntoView {
    let query = request_query();
    let is_completed = query.status.as_deref() == Some("completed");

    view! {
        <AuthLayout title="Reset Password" eyebrow="Access Recovery">
            {if is_completed {
                view! {
                    <div class="flex flex-col gap-6">
                        <div class="border border-[#196c4a]/20 bg-[#196c4a]/10 text-[#196c4a] p-4 rounded-xl text-sm flex items-start gap-2 dark:bg-[#196c4a]/5">
                            <span class="font-bold">"Success:"</span>
                            <span>"Your password has been reset successfully. You can now use your new credentials to log in."</span>
                        </div>
                        <a
                            href="/login"
                            class="w-full py-3.5 px-4 bg-accent hover:bg-accent/95 text-white font-bold rounded-xl text-center decoration-transparent transition-all shadow-md shadow-accent/15"
                        >
                            "Sign In Now"
                        </a>
                    </div>
                }.into_any()
            } else {
                view! {
                    <form method="get" action="/reset-password" class="flex flex-col gap-5">
                        <input type="hidden" name="status" value="completed"/>
                        <p class="text-sm text-[#527064] font-medium leading-relaxed">
                            "Please choose a strong password that you do not use on other platforms to secure your administrative dashboard."
                        </p>
                        <label class="flex flex-col gap-2 text-sm font-bold text-ink">
                            "New Password"
                            <input
                                name="password"
                                type="password"
                                autocomplete="new-password"
                                required
                                class="w-full border border-input-border rounded-xl p-3 bg-panel-bg text-ink focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent transition-all"
                                placeholder="••••••••"
                            />
                        </label>
                        <label class="flex flex-col gap-2 text-sm font-bold text-ink">
                            "Confirm New Password"
                            <input
                                name="confirm_password"
                                type="password"
                                autocomplete="new-password"
                                required
                                class="w-full border border-input-border rounded-xl p-3 bg-panel-bg text-ink focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent transition-all"
                                placeholder="••••••••"
                            />
                        </label>
                        <button
                            type="submit"
                            class="w-full py-3.5 px-4 bg-accent hover:bg-accent/95 text-white font-bold rounded-xl transition-all shadow-md shadow-accent/15 cursor-pointer mt-2"
                        >
                            "Reset Password"
                        </button>
                    </form>
                }.into_any()
            }}
        </AuthLayout>
    }
}
