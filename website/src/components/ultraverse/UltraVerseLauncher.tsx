import Link from "next/link";
import {
  ArrowDownRight,
  ArrowRight,
  ArrowUpRight,
  ClipboardCheck,
  Layers3,
  Wallet,
} from "lucide-react";
import styles from "./UltraVerseLauncher.module.css";
import { conceptGroups, supportedPaths, type SupportedPathIcon } from "@/lib/ultraverse";

const pathIcons = {
  wallet: Wallet,
  layers: Layers3,
  validator: ClipboardCheck,
} satisfies Record<SupportedPathIcon, typeof Wallet>;

export default function UltraVerseLauncher() {
  return (
    <main className={styles.page} data-page-shell="ultraverse">
      <div className={styles.shell}>
        <header className={styles.header} aria-label="UltraVerse">
          <Link className={styles.brand} href="/ultraverse" aria-label="UltraVerse services">
            <span className={styles.brandMark} aria-hidden="true">U</span>
            <span className={styles.brandCopy}>
              <span className={styles.brandName}>UltraVerse</span>
              <span className={styles.brandSub}>on UltraNet</span>
            </span>
          </Link>
          <nav className={styles.nav} aria-label="Directory navigation">
            {/* Both targets are sections of this page, so "location" is the correct token;
                aria-current="page" claims the link points at the document itself. */}
            <a href="#services" aria-current="location">Services</a>
            <a href="#concepts">Concepts</a>
          </nav>
        </header>

        <section className={styles.intro} aria-labelledby="ultraverse-title">
          <div>
            <h1 id="ultraverse-title">Services on UltraNet</h1>
            <p>Choose a supported path. Each has a different access requirement.</p>
          </div>
          <a className={styles.conceptJump} href="#concepts">
            Browse concepts <ArrowDownRight size={17} aria-hidden="true" />
          </a>
        </section>

        <section id="services" aria-labelledby="supported-paths-title" className={styles.services}>
          <div className={styles.sectionHeading}>
            <h2 id="supported-paths-title">Supported paths</h2>
            <p>Three ways into the current product</p>
          </div>

          <div className={styles.pathGrid}>
            {supportedPaths.map((path) => {
              const Icon = pathIcons[path.icon];
              const isWallet = path.id === "wallet";
              const isValidator = path.id === "validator";

              return (
                <article
                  key={path.id}
                  className={`${styles.path} ${isWallet ? styles.wallet : ""} ${path.id === "appchains" ? styles.appchain : ""} ${isValidator ? styles.validator : ""}`}
                  aria-labelledby={`${path.id}-title`}
                >
                  <div className={styles.pathHead}>
                    <span className={styles.pathIcon} aria-hidden="true"><Icon size={23} strokeWidth={2} /></span>
                    <h3 id={`${path.id}-title`}>{path.title}</h3>
                  </div>

                  <p className={styles.access}>
                    <span className={styles.accessDot} aria-hidden="true" />
                    {path.accessLabel}
                  </p>

                  {isWallet ? (
                    <>
                      <div className={styles.transferFlow} aria-label="Review transfer, then sign locally">
                        <span>Review transfer</span><ArrowRight size={14} aria-hidden="true" /><span>Sign locally</span>
                      </div>
                      <p className={styles.walletNote}>Receive by copying your address <span aria-hidden="true">·</span> no QR flow.</p>
                    </>
                  ) : isValidator ? (
                    <>
                      <p className={styles.validatorFact}>Activation requires approval from <strong>two distinct Sovereign owners.</strong></p>
                      <div className={styles.validatorFlow} aria-label="Wallet-signed proposal, approval from two distinct Sovereign owners, then activation">
                        <span>Wallet-signed proposal</span>
                        <ArrowRight size={14} aria-hidden="true" />
                        <span>2 distinct Sovereign owners approve</span>
                        <ArrowRight size={14} aria-hidden="true" />
                        <span>Activation</span>
                      </div>
                      <p className={styles.secondaryFact}>{path.facts[1]}</p>
                    </>
                  ) : (
                    <ul className={styles.factList}>
                      {path.facts.map((fact) => <li key={fact}>{fact}</li>)}
                    </ul>
                  )}

                  <div className={styles.actions}>
                    <Link className={styles.primaryAction} href={path.href}>
                      {path.actionLabel}<ArrowUpRight size={16} aria-hidden="true" />
                    </Link>
                    {path.secondaryActions && (
                      <div className={styles.secondaryActions}>
                        {path.secondaryActions.map((action) => (
                          <Link key={action.label} className={styles.textAction} href={action.href}>{action.label}</Link>
                        ))}
                      </div>
                    )}
                  </div>
                </article>
              );
            })}
          </div>
        </section>

        <section id="concepts" className={styles.concepts} aria-labelledby="concepts-title">
          <div className={styles.conceptHeading}>
            <div>
              <h2 id="concepts-title">Concepts · not implemented</h2>
              <p>Ecosystem ideas, shown separately from supported paths.</p>
            </div>
            <span className={styles.conceptMark}><span aria-hidden="true">◇</span> Not available services</span>
          </div>

          <div className={styles.conceptGrid}>
            {conceptGroups.map((group) => (
              <section className={styles.conceptGroup} key={group.id} aria-labelledby={`concept-${group.id}`}>
                <h3 id={`concept-${group.id}`}>{group.title}</h3>
                <ul className={styles.conceptList}>
                  {group.items.map((item) => (
                    <li className={styles.conceptItem} key={item}>
                      <span className={styles.conceptDot} aria-hidden="true" />
                      <span>{item}</span>
                      <span className={styles.conceptLabel}>Concept</span>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
          <p className={styles.conceptFoot}>Groups describe ideas only; they do not indicate available integrations or launch timing.</p>
        </section>
      </div>
    </main>
  );
}
