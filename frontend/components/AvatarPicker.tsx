"use client";

import { ArrowsClockwise, Check, Sparkle } from "@phosphor-icons/react";
import { Avatar, AvatarBytes, randomAvatar } from "./Avatar";
import styles from "./ndraw.module.css";

export function AvatarPicker({
  name,
  value,
  onChange,
}: {
  name: string;
  value: AvatarBytes;
  onChange: (avatar: AvatarBytes) => void;
}) {
  const candidates = Array.from({ length: 6 }, (_, index) => {
    const candidate = [...value] as number[];
    candidate[0] = (candidate[0] + index * 37) % 256;
    candidate[2] = (candidate[2] + index * 61) % 256;
    candidate[6] = (candidate[6] + index) % 256;
    return candidate as unknown as AvatarBytes;
  });

  return (
    <section className={styles.avatarPicker} aria-labelledby="avatar-heading">
      <div className={styles.avatarPickerHeading}>
        <div>
          <span className={styles.eyebrow}>Your tiny alter ego</span>
          <h2 id="avatar-heading">Pick a face</h2>
        </div>
        <button className={styles.tertiaryButton} onClick={() => onChange(randomAvatar())} type="button">
          <ArrowsClockwise size={17} weight="bold" /> Shuffle
        </button>
      </div>
      <div className={styles.avatarGrid}>
        {candidates.map((candidate, index) => {
          const selected = candidate.every((byte, byteIndex) => byte === value[byteIndex]);
          return (
            <button
              aria-label={`Choose avatar variation ${index + 1}`}
              className={styles.avatarChoice}
              data-selected={selected}
              key={candidate.join("-")}
              onClick={() => onChange(candidate)}
              type="button"
            >
              <Avatar name={name} size={62} value={candidate} />
              {selected ? <span className={styles.avatarCheck}><Check size={12} weight="bold" /></span> : null}
            </button>
          );
        })}
      </div>
      <p className={styles.avatarNote}><Sparkle size={15} weight="fill" /> Your avatar is generated on-device and stays stable between reconnects.</p>
    </section>
  );
}
