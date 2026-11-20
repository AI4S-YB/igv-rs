#!/usr/bin/env python3
"""
Generate reproducible genomics demo fixtures in `examples/demo/` and record
7 Full-HD (1920x1080) narrated MP4 tutorials + GIF previews + poster PNGs
in `docs/videos/` from live `igv-rs` PTY sessions with on-screen keystroke overlays.
"""

from __future__ import annotations

import fcntl
import math
import os
import pty
import re
import select
import shutil
import socket
import struct
import subprocess
import tempfile
import termios
import time
import wave
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Optional

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
DEMO_DIR = ROOT / "examples" / "demo"
VIDEO_DIR = ROOT / "docs" / "videos"
BIN_PATH = ROOT / "target" / "release" / "igv-rs"
if not BIN_PATH.exists():
    BIN_PATH = ROOT / "target" / "debug" / "igv-rs"

# Terminal PTY geometry and 1920x1080 pixel layout
COLS = 124
ROWS = 38
CELL_W = 14
CELL_H = 20
FRAME_W = 1920
FRAME_H = 1080
FPS = 24

WIN_X0, WIN_Y0 = 32, 66
WIN_X1, WIN_Y1 = 1888, 878
GRID_X = (FRAME_W - COLS * CELL_W) // 2  # 92
GRID_Y = 104  # 104 + 38 * 20 = 864

# Colors
BG_CANVAS = (11, 15, 23)
BG_TERM = (15, 20, 29)
FG_DEFAULT = (226, 232, 240)
BG_CHROME = (22, 29, 43)
BORDER_WIN = (51, 65, 85)
ACCENT_CYAN = (56, 189, 248)
ACCENT_AMBER = (245, 158, 11)
ACCENT_EMERALD = (52, 211, 153)

MENLO_PATH = "/System/Library/Fonts/Menlo.ttc"
SANS_PATH = "/System/Library/Fonts/Supplemental/Arial Bold.ttf"
SANS_REG_PATH = "/System/Library/Fonts/Supplemental/Arial.ttf"


# ---------------------------------------------------------------------------
# 1. Demo Genomics Fixtures Generator (examples/demo/)
# ---------------------------------------------------------------------------


def build_demo_fixtures() -> None:
    DEMO_DIR.mkdir(parents=True, exist_ok=True)

    # Construct chr1 (1200 bp) and chr2 (800 bp) with realistic motifs:
    # - TATAAAA at 125
    # - ATG start at 140
    # - Heterozygous A/G SNP site at 150 (ref='A')
    # - CpG islands at 170, 182, 196, 210, 224, 238, 250
    # - EcoRI GAATTC at 240
    # - TP53 region around 620..1080
    unit = "ACGTCTGACTGACTGATCGTAGCTAGCATG"
    chr1_list = list((unit * ((1200 // len(unit)) + 2))[:1200])
    chr2_list = list(("GCTAGTCAGTCGTACGATCGTAGCATCGTA" * 35)[:800])

    def patch(seq: list[str], start_1based: int, sub: str) -> None:
        for i, ch in enumerate(sub):
            pos = start_1based - 1 + i
            if 0 <= pos < len(seq):
                seq[pos] = ch

    patch(chr1_list, 125, "TATAAAA")
    patch(chr1_list, 140, "ATGCCAGAC")
    patch(chr1_list, 150, "A")  # Ref = A at chr1:150
    patch(chr1_list, 165, "CGCGATCGCGATCGCGATCG")
    patch(chr1_list, 195, "AGACGCGTTACGCGTAGCATCG")
    patch(chr1_list, 225, "ACGTACGTACGTA")
    patch(chr1_list, 240, "GAATTCTAA")  # GAATTC + stop TAA
    patch(chr1_list, 300, "TATAAAACGCGATGCCAGATTGA")
    patch(chr1_list, 650, "ATGGAGGAGCCGCAGTCAGATCCTAGCG")
    patch(chr1_list, 830, "CGCGATATAAAAAGGCCCGAATTC")
    patch(chr2_list, 120, "ATGCGACCCTGGAGAATTCACGT")

    chr1_seq = "".join(chr1_list)
    chr2_seq = "".join(chr2_list)

    # demo.fa + demo.fa.fai
    fa_path = DEMO_DIR / "demo.fa"
    fa_lines = [">chr1\n"]
    for i in range(0, len(chr1_seq), 60):
        fa_lines.append(chr1_seq[i : i + 60] + "\n")
    chr2_offset = sum(len(l.encode("ascii")) for l in fa_lines) + len(">chr2\n")
    fa_lines.append(">chr2\n")
    for i in range(0, len(chr2_seq), 60):
        fa_lines.append(chr2_seq[i : i + 60] + "\n")
    fa_path.write_text("".join(fa_lines), encoding="ascii")

    (DEMO_DIR / "demo.fa.fai").write_text(
        f"chr1\t{len(chr1_seq)}\t6\t60\t61\n"
        f"chr2\t{len(chr2_seq)}\t{chr2_offset}\t60\t61\n",
        encoding="ascii",
    )

    # genes.gtf
    gtf = "\n".join(
        [
            'chr1\tshowcase\tgene\t60\t520\t.\t+\t.\tgene_id "ENSG00001"; gene_name "BRCA1";',
            'chr1\tshowcase\ttranscript\t60\t520\t.\t+\t.\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\texon\t60\t190\t.\t+\t.\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\tCDS\t140\t190\t.\t+\t0\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\texon\t260\t360\t.\t+\t.\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\tCDS\t260\t360\t.\t+\t0\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\texon\t430\t520\t.\t+\t.\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\tCDS\t430\t500\t.\t+\t0\tgene_id "ENSG00001"; transcript_id "ENST00001"; gene_name "BRCA1";',
            'chr1\tshowcase\tgene\t620\t1080\t.\t-\t.\tgene_id "ENSG00002"; gene_name "TP53";',
            'chr1\tshowcase\ttranscript\t620\t1080\t.\t-\t.\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr1\tshowcase\texon\t620\t740\t.\t-\t.\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr1\tshowcase\tCDS\t650\t740\t.\t-\t0\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr1\tshowcase\texon\t820\t940\t.\t-\t.\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr1\tshowcase\tCDS\t820\t940\t.\t-\t0\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr1\tshowcase\texon\t1000\t1080\t.\t-\t.\tgene_id "ENSG00002"; transcript_id "ENST00002"; gene_name "TP53";',
            'chr2\tshowcase\tgene\t80\t680\t.\t+\t.\tgene_id "ENSG00003"; gene_name "EGFR";',
            'chr2\tshowcase\ttranscript\t80\t680\t.\t+\t.\tgene_id "ENSG00003"; transcript_id "ENST00003"; gene_name "EGFR";',
            'chr2\tshowcase\texon\t80\t280\t.\t+\t.\tgene_id "ENSG00003"; transcript_id "ENST00003"; gene_name "EGFR";',
            'chr2\tshowcase\tCDS\t120\t280\t.\t+\t0\tgene_id "ENSG00003"; transcript_id "ENST00003"; gene_name "EGFR";',
            'chr2\tshowcase\texon\t420\t680\t.\t+\t.\tgene_id "ENSG00003"; transcript_id "ENST00003"; gene_name "EGFR";',
            'chr2\tshowcase\tCDS\t420\t640\t.\t+\t0\tgene_id "ENSG00003"; transcript_id "ENST00003"; gene_name "EGFR";',
        ]
    )
    (DEMO_DIR / "genes.gtf").write_text(gtf + "\n", encoding="ascii")

    # reads.sam with paired reads, Het G/A SNV at 150, HP tags, RG tags, MM/ML 5mC mods,
    # soft clips, discordant & chimeric mates, and spliced RNA-seq reads for Sashimi.
    sam_lines = [
        "@HD\tVN:1.6\tSO:coordinate",
        "@SQ\tSN:chr1\tLN:1200",
        "@SQ\tSN:chr2\tLN:800",
        "@RG\tID:tumor\tSM:Tumor_01",
        "@RG\tID:normal\tSM:Normal_01",
    ]

    def subseq(chrom: str, start1: int, length: int, mut_150: Optional[str] = None) -> str:
        base = chr1_seq if chrom == "chr1" else chr2_seq
        s = list(base[start1 - 1 : start1 - 1 + length])
        if chrom == "chr1" and mut_150 is not None:
            idx = 150 - start1
            if 0 <= idx < len(s):
                s[idx] = mut_150
        return "".join(s)

    def mm_ml_for_seq(seq: str, is_rev: bool) -> tuple[str, str]:
        # Mark every C (or G on reverse strand) as 5mC methylated with varying probabilities
        orient = "".join(
            {"A": "T", "T": "A", "C": "G", "G": "C"}.get(b, "N") for b in reversed(seq)
        ) if is_rev else seq
        c_count = sum(1 for b in orient if b == "C")
        if c_count == 0:
            return "MM:Z:C+m;", "ML:B:C"
        deltas = ["0"] * min(c_count, 5)
        probs = [245 if (i % 2 == 0) else 20 for i in range(len(deltas))]
        mm = "MM:Z:C+m," + ",".join(deltas) + ";"
        ml = "ML:B:C," + ",".join(str(p) for p in probs)
        return mm, ml

    reads = []
    # Pair 1-4 covering chr1:110..270 with mixed Het A/G at 150 + MM/ML + HP tags
    pair_specs = [
        ("pair_01", 110, 210, 48, "A", 1, "tumor", 60, False),
        ("pair_02", 118, 222, 48, "G", 2, "tumor", 60, False),
        ("pair_03", 125, 228, 48, "G", 2, "tumor", 55, False),
        ("pair_04", 132, 236, 48, "A", 1, "normal", 60, False),
        ("pair_05", 138, 245, 48, "G", 2, "normal", 42, False),
        ("pair_06", 142, 250, 48, "A", 1, "normal", 18, True),  # Soft clipped
    ]
    for qname, p1, p2, rlen, alt150, hp, rg, mapq, clip in pair_specs:
        # Mate 1 forward
        seq1 = subseq("chr1", p1, rlen, mut_150=alt150)
        if clip:
            cigar1 = f"6S{rlen - 6}M"
            seq1 = "TTTTTT" + seq1[6:]
        else:
            cigar1 = f"{rlen}M"
        qual1 = "I" * len(seq1)
        mm1, ml1 = mm_ml_for_seq(seq1, False)
        tlen = (p2 + rlen) - p1
        reads.append(
            (p1, f"{qname}\t99\tchr1\t{p1}\t{mapq}\t{cigar1}\t=\t{p2}\t{tlen}\t{seq1}\t{qual1}\tHP:i:{hp}\tRG:Z:{rg}\t{mm1}\t{ml1}")
        )
        # Mate 2 reverse
        seq2 = subseq("chr1", p2, rlen, mut_150=alt150)
        qual2 = "I" * len(seq2)
        mm2, ml2 = mm_ml_for_seq(seq2, True)
        reads.append(
            (p2, f"{qname}\t147\tchr1\t{p2}\t{mapq}\t{rlen}M\t=\t{p1}\t{-tlen}\t{seq2}\t{qual2}\tHP:i:{hp}\tRG:Z:{rg}\t{mm2}\t{ml2}")
        )

    # Discordant insert-size pair + chimeric chr2 mate read
    dseq = subseq("chr1", 105, 50, mut_150="G")
    dmm, dml = mm_ml_for_seq(dseq, False)
    reads.append((105, f"disc_01\t97\tchr1\t105\t35\t50M\t=\t590\t535\t{dseq}\t{'I'*50}\tHP:i:2\tRG:Z:tumor\t{dmm}\t{dml}"))
    cseq = subseq("chr1", 150, 44, mut_150="A")
    cmm, cml = mm_ml_for_seq(cseq, False)
    reads.append((150, f"chim_01\t65\tchr1\t150\t28\t6S38M\tchr2\t200\t0\t{'AAAAAA'+cseq[6:]}\t{'I'*44}\tHP:i:1\tRG:Z:tumor\t{cmm}\t{cml}"))

    # Spliced RNA-seq reads across BRCA1 introns (exon1 ends 190 -> exon2 starts 260 => 69N intron skip;
    # exon2 ends 360 -> exon3 starts 430 => 69N intron skip)
    for idx in range(1, 7):
        pos = 152 + idx
        left_len = 191 - pos
        right_len = 32
        seq_spl = chr1_seq[pos - 1 : pos - 1 + left_len] + chr1_seq[259 : 259 + right_len]
        mm_s, ml_s = mm_ml_for_seq(seq_spl, idx % 2 == 1)
        flag_s = 16 if idx % 2 == 1 else 0
        reads.append(
            (pos, f"rna_spl1_{idx}\t{flag_s}\tchr1\t{pos}\t60\t{left_len}M69N{right_len}M\t*\t0\t0\t{seq_spl}\t{'I'*len(seq_spl)}\tHP:i:{1 + idx%2}\tRG:Z:tumor\t{mm_s}\t{ml_s}")
        )
    for idx in range(1, 5):
        pos = 326 + idx
        left_len = 361 - pos
        right_len = 30
        seq_spl = chr1_seq[pos - 1 : pos - 1 + left_len] + chr1_seq[429 : 429 + right_len]
        mm_s, ml_s = mm_ml_for_seq(seq_spl, idx % 2 == 0)
        flag_s = 16 if idx % 2 == 0 else 0
        reads.append(
            (pos, f"rna_spl2_{idx}\t{flag_s}\tchr1\t{pos}\t60\t{left_len}M69N{right_len}M\t*\t0\t0\t{seq_spl}\t{'I'*len(seq_spl)}\tHP:i:{1 + idx%2}\tRG:Z:normal\t{mm_s}\t{ml_s}")
        )
    # Reads at TP53 (chr1:650..740) and chr2 (120..220)
    for idx, pos in enumerate([640, 655, 670, 690]):
        seq_tp = subseq("chr1", pos, 46)
        reads.append((pos, f"tp53_r{idx}\t0\tchr1\t{pos}\t60\t46M\t*\t0\t0\t{seq_tp}\t{'I'*46}\tRG:Z:tumor"))

    reads.sort(key=lambda r: r[0])
    sam_lines.extend(r[1] for r in reads)
    (DEMO_DIR / "reads.sam").write_text("\n".join(sam_lines) + "\n", encoding="ascii")

    # trio.ped & trio.vcf (Father, Mother, Proband with Mendelian violation)
    (DEMO_DIR / "trio.ped").write_text(
        "FAM1\tFather\t0\t0\t1\t1\n"
        "FAM1\tMother\t0\t0\t2\t1\n"
        "FAM1\tProband\tFather\tMother\t1\t2\n",
        encoding="ascii",
    )
    vcf_lines = [
        "##fileformat=VCFv4.2",
        "##contig=<ID=chr1,length=1200>",
        "##contig=<ID=chr2,length=800>",
        '##INFO=<ID=AF,Number=A,Type=Float,Description="Allele Frequency">',
        '##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">',
        '##FORMAT=<ID=DP,Number=1,Type=Integer,Description="Read Depth">',
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tFather\tMother\tProband",
        "chr1\t112\trs1001\tC\tT\t95\tPASS\tAF=0.33\tGT:DP\t0/1:32\t0/0:29\t0/1:35",
        "chr1\t150\trs1002\tA\tG\t99\tPASS\tAF=0.50\tGT:DP\t0/1:44\t0/1:41\t1/1:48",
        "chr1\t205\trs1003\tG\tA\t88\tPASS\tAF=0.17\tGT:DP\t0/0:38\t0/0:40\t0/1:39",  # Mendelian violation
        "chr1\t255\trs1004\tT\tC\t76\tPASS\tAF=0.33\tGT:DP\t0/0:24\t0/1:28\t0/1:31",
        "chr1\t320\trs1005\tA\tT\t92\tPASS\tAF=0.50\tGT:DP\t1/1:35\t0/0:33\t0/1:34",
        "chr1\t680\trs2001\tG\tC\t99\tPASS\tAF=0.33\tGT:DP\t0/0:30\t0/1:31\t0/1:29",
        "chr2\t180\trs3001\tC\tA\t91\tPASS\tAF=0.33\tGT:DP\t0/1:27\t0/0:26\t0/1:30",
    ]
    (DEMO_DIR / "trio.vcf").write_text("\n".join(vcf_lines) + "\n", encoding="ascii")

    # chip_h3k27ac.wig & atac_seq.bedGraph
    wig_lines = ["track type=wiggle_0 name=H3K27ac", "variableStep chrom=chr1 span=10"]
    for pos in range(40, 1180, 10):
        val = (
            12.0 * math.exp(-((pos - 160) ** 2) / 3600.0)
            + 9.5 * math.exp(-((pos - 440) ** 2) / 3200.0)
            + 11.0 * math.exp(-((pos - 720) ** 2) / 4000.0)
            + 1.2
        )
        wig_lines.append(f"{pos}\t{val:.2f}")
    wig_lines.append("variableStep chrom=chr2 span=10")
    for pos in range(40, 760, 10):
        val = 10.0 * math.exp(-((pos - 220) ** 2) / 4500.0) + 1.0
        wig_lines.append(f"{pos}\t{val:.2f}")
    (DEMO_DIR / "chip_h3k27ac.wig").write_text("\n".join(wig_lines) + "\n", encoding="ascii")

    bg_lines = ["track type=bedGraph name=ATAC-seq"]
    for start in range(20, 1160, 20):
        end = start + 20
        val = 8.0 * math.exp(-((start - 200) ** 2) / 5000.0) + 7.0 * math.exp(-((start - 660) ** 2) / 4000.0) + 0.8
        bg_lines.append(f"chr1\t{start}\t{end}\t{val:.2f}")
    for start in range(20, 740, 20):
        bg_lines.append(f"chr2\t{start}\t{start + 20}\t{2.5 + 3.0 * math.sin(start / 80.0):.2f}")
    (DEMO_DIR / "atac_seq.bedGraph").write_text("\n".join(bg_lines) + "\n", encoding="ascii")

    # loops.bedpe
    bedpe_lines = [
        "chr1\t80\t120\tchr1\t280\t320\tloop_enhancer_1\t14.5\t+\t-",
        "chr1\t150\t190\tchr1\t430\t470\tloop_promoter\t9.2\t+\t-",
        "chr1\t260\t300\tchr1\t650\t690\tlong_range_arc\t11.8\t+\t-",
        "chr1\t100\t140\tchr2\t180\t220\ttrans_fusion_sv\t12.0\t+\t+",
    ]
    (DEMO_DIR / "loops.bedpe").write_text("\n".join(bedpe_lines) + "\n", encoding="ascii")

    # cohort_cnv.seg
    seg_lines = [
        "ID\tchrom\tloc.start\tloc.end\tnum.mark\tseg.mean",
        "Tumor_01\tchr1\t1\t120\t20\t-0.15",
        "Tumor_01\tchr1\t121\t340\t42\t1.45",
        "Tumor_01\tchr1\t341\t620\t36\t0.10",
        "Tumor_01\tchr1\t621\t980\t48\t-1.35",
        "Tumor_01\tchr1\t981\t1200\t22\t0.05",
        "Tumor_01\tchr2\t1\t420\t50\t0.95",
        "Tumor_02\tchr1\t1\t180\t25\t0.18",
        "Tumor_02\tchr1\t181\t460\t44\t1.15",
        "Tumor_02\tchr1\t461\t900\t52\t-0.95",
        "Tumor_02\tchr1\t901\t1200\t30\t-0.12",
        "Tumor_03\tchr1\t1\t260\t30\t-1.10",
        "Tumor_03\tchr1\t261\t520\t38\t0.85",
        "Tumor_03\tchr1\t521\t1200\t60\t-0.25",
        "Normal_01\tchr1\t1\t1200\t120\t0.02",
        "Normal_01\tchr2\t1\t800\t80\t-0.01",
    ]
    (DEMO_DIR / "cohort_cnv.seg").write_text("\n".join(seg_lines) + "\n", encoding="ascii")

    # cardiometabolic.gwas
    gwas_lines = ["SNP\tCHR\tBP\tP"]
    for i, bp in enumerate(range(15, 1190, 12)):
        if 175 <= bp <= 265:
            dist = abs(bp - 220)
            pval = 1.2e-11 * (10.0 ** (dist / 12.0))
        elif 660 <= bp <= 740:
            dist = abs(bp - 700)
            pval = 3.5e-7 * (10.0 ** (dist / 18.0))
        else:
            pval = 0.08 + 0.7 * ((i * 37) % 100) / 100.0
        pval = min(max(pval, 1e-12), 0.99)
        gwas_lines.append(f"rs{5000 + i}\tchr1\t{bp}\t{pval:.3e}")
    for i, bp in enumerate(range(20, 780, 20)):
        pval = 1e-5 if abs(bp - 220) < 20 else 0.15 + 0.5 * ((i * 23) % 100) / 100.0
        gwas_lines.append(f"rs{7000 + i}\tchr2\t{bp}\t{pval:.3e}")
    (DEMO_DIR / "cardiometabolic.gwas").write_text("\n".join(gwas_lines) + "\n", encoding="ascii")

    # batch_example.igv
    (DEMO_DIR / "batch_example.igv").write_text(
        "# Headless batch script for igv-rs\n"
        "new\n"
        "genome examples/demo/demo.fa\n"
        "load examples/demo/genes.gtf examples/demo/reads.sam examples/demo/trio.vcf\n"
        "snapshotDirectory /tmp/igv_demo_out\n"
        "goto chr1:100-340\n"
        "colorBy mod\n"
        "sashimi on\n"
        "snapshot brca1_region.svg\n"
        "goto TP53\n"
        "snapshot tp53_region.png\n"
        "exit\n",
        encoding="ascii",
    )


# ---------------------------------------------------------------------------
# 2. VT100 + 24-bit TrueColor Terminal Emulator
# ---------------------------------------------------------------------------


def build_xterm_256_palette() -> list[tuple[int, int, int]]:
    pal = [
        (15, 20, 29),
        (239, 68, 68),
        (34, 197, 94),
        (234, 179, 8),
        (59, 130, 246),
        (217, 70, 239),
        (6, 182, 212),
        (203, 213, 225),
        (100, 116, 139),
        (248, 113, 113),
        (74, 222, 128),
        (250, 204, 21),
        (96, 165, 250),
        (232, 121, 249),
        (34, 211, 238),
        (248, 250, 252),
    ]
    steps = [0, 95, 135, 175, 215, 255]
    for r in steps:
        for g in steps:
            for b in steps:
                pal.append((r, g, b))
    for i in range(24):
        v = 8 + i * 10
        pal.append((v, v, v))
    return pal


PALETTE_256 = build_xterm_256_palette()


@dataclass
class Cell:
    ch: str = " "
    fg: tuple[int, int, int] = FG_DEFAULT
    bg: tuple[int, int, int] = BG_TERM
    bold: bool = False
    dim: bool = False
    rev: bool = False


class VT100Screen:
    def __init__(self, cols: int = COLS, rows: int = ROWS) -> None:
        self.cols = cols
        self.rows = rows
        self.grid = [[Cell() for _ in range(cols)] for _ in range(rows)]
        self.r = 0
        self.c = 0
        self.fg = FG_DEFAULT
        self.bg = BG_TERM
        self.bold = False
        self.dim = False
        self.rev = False
        self._buf = ""
        self._byte_tail = b""
        self.dirty = True
        self.freeze_alt_exit = False

    def blank_cell(self) -> Cell:
        return Cell(" ", self.fg, self.bg, False, False, False)

    def feed_bytes(self, data: bytes) -> None:
        combined = self._byte_tail + data
        # Safely split off trailing incomplete UTF-8 sequence
        cut = len(combined)
        for back in range(1, min(4, len(combined)) + 1):
            b = combined[-back]
            if b < 0x80:
                break
            if (b & 0xC0) == 0xC0:
                expected = 2 if (b & 0xE0) == 0xC0 else 3 if (b & 0xF0) == 0xE0 else 4
                if back < expected:
                    cut = len(combined) - back
                break
        self._byte_tail = combined[cut:]
        text = combined[:cut].decode("utf-8", errors="replace")
        self._process_text(text)

    def _process_text(self, text: str) -> None:
        s = self._buf + text
        self._buf = ""
        i = 0
        n = len(s)
        while i < n:
            ch = s[i]
            if ch == "\x1b":
                if i + 1 >= n:
                    self._buf = s[i:]
                    return
                nxt = s[i + 1]
                if nxt == "[":
                    # Find CSI final byte in 0x40..0x7E
                    j = i + 2
                    while j < n and not (0x40 <= ord(s[j]) <= 0x7E):
                        j += 1
                    if j >= n:
                        self._buf = s[i:]
                        return
                    self._handle_csi(s[i + 2 : j], s[j])
                    i = j + 1
                    continue
                if nxt == "]":
                    bel = s.find("\x07", i + 2)
                    st = s.find("\x1b\\", i + 2)
                    if bel == -1 and st == -1:
                        self._buf = s[i:]
                        return
                    end = bel + 1 if (bel != -1 and (st == -1 or bel < st)) else st + 2
                    i = end
                    continue
                if nxt in ("(", ")", "*", "+"):
                    if i + 2 >= n:
                        self._buf = s[i:]
                        return
                    i += 3
                    continue
                i += 2
                continue
            if ch == "\r":
                self.c = 0
            elif ch == "\n":
                self.r = min(self.rows - 1, self.r + 1)
            elif ch == "\b":
                self.c = max(0, self.c - 1)
            elif ch == "\t":
                self.c = min(self.cols - 1, ((self.c // 8) + 1) * 8)
            elif ord(ch) >= 32:
                if 0 <= self.r < self.rows and 0 <= self.c < self.cols:
                    self.grid[self.r][self.c] = Cell(ch, self.fg, self.bg, self.bold, self.dim, self.rev)
                    self.dirty = True
                if self.c < self.cols - 1:
                    self.c += 1
            i += 1

    def _parse_params(self, raw: str) -> list[int]:
        if not raw:
            return []
        clean = raw.lstrip("?><!")
        out = []
        for p in clean.split(";"):
            sub = p.split(":")[0]
            try:
                out.append(int(sub) if sub else 0)
            except ValueError:
                out.append(0)
        return out

    def _handle_csi(self, raw: str, final: str) -> None:
        if raw.startswith("?"):
            if final in ("h", "l"):
                if "1049" in raw and final == "l":
                    self.freeze_alt_exit = True
                return
            return
        if self.freeze_alt_exit:
            return
        params = self._parse_params(raw)
        p0 = params[0] if params else 0
        if final in ("H", "f"):
            row = (params[0] if len(params) >= 1 and params[0] > 0 else 1) - 1
            col = (params[1] if len(params) >= 2 and params[1] > 0 else 1) - 1
            self.r = max(0, min(self.rows - 1, row))
            self.c = max(0, min(self.cols - 1, col))
        elif final == "A":
            self.r = max(0, self.r - max(1, p0))
        elif final == "B":
            self.r = min(self.rows - 1, self.r + max(1, p0))
        elif final == "C":
            self.c = min(self.cols - 1, self.c + max(1, p0))
        elif final == "D":
            self.c = max(0, self.c - max(1, p0))
        elif final == "G":
            self.c = max(0, min(self.cols - 1, max(1, p0) - 1))
        elif final == "d":
            self.r = max(0, min(self.rows - 1, max(1, p0) - 1))
        elif final == "J":
            if p0 in (2, 3):
                for rr in range(self.rows):
                    for cc in range(self.cols):
                        self.grid[rr][cc] = self.blank_cell()
            elif p0 == 0:
                for cc in range(self.c, self.cols):
                    self.grid[self.r][cc] = self.blank_cell()
                for rr in range(self.r + 1, self.rows):
                    for cc in range(self.cols):
                        self.grid[rr][cc] = self.blank_cell()
            self.dirty = True
        elif final == "K":
            if p0 == 0:
                for cc in range(self.c, self.cols):
                    self.grid[self.r][cc] = self.blank_cell()
            elif p0 == 1:
                for cc in range(0, self.c + 1):
                    self.grid[self.r][cc] = self.blank_cell()
            elif p0 == 2:
                for cc in range(self.cols):
                    self.grid[self.r][cc] = self.blank_cell()
            self.dirty = True
        elif final == "X":
            count = max(1, p0)
            for cc in range(self.c, min(self.cols, self.c + count)):
                self.grid[self.r][cc] = self.blank_cell()
            self.dirty = True
        elif final == "m":
            self._handle_sgr(raw)

    def _handle_sgr(self, raw: str) -> None:
        if not raw:
            self.fg, self.bg, self.bold, self.dim, self.rev = FG_DEFAULT, BG_TERM, False, False, False
            return
        # Handle colon or semicolon sequences uniformly
        tokens: list[int] = []
        for part in raw.split(";"):
            for sub in part.split(":"):
                try:
                    tokens.append(int(sub) if sub else 0)
                except ValueError:
                    tokens.append(0)
        i = 0
        while i < len(tokens):
            code = tokens[i]
            if code == 0:
                self.fg, self.bg = FG_DEFAULT, BG_TERM
                self.bold = self.dim = self.rev = False
            elif code == 1:
                self.bold = True
            elif code == 2:
                self.dim = True
            elif code == 7:
                self.rev = True
            elif code == 22:
                self.bold = self.dim = False
            elif code == 27:
                self.rev = False
            elif 30 <= code <= 37:
                self.fg = PALETTE_256[code - 30]
            elif code == 39:
                self.fg = FG_DEFAULT
            elif 40 <= code <= 47:
                self.bg = PALETTE_256[code - 40]
            elif code == 49:
                self.bg = BG_TERM
            elif 90 <= code <= 97:
                self.fg = PALETTE_256[8 + (code - 90)]
            elif 100 <= code <= 107:
                self.bg = PALETTE_256[8 + (code - 100)]
            elif code in (38, 48) and i + 1 < len(tokens):
                is_fg = code == 38
                mode = tokens[i + 1]
                if mode == 5 and i + 2 < len(tokens):
                    idx = max(0, min(255, tokens[i + 2]))
                    if is_fg:
                        self.fg = PALETTE_256[idx]
                    else:
                        self.bg = PALETTE_256[idx]
                    i += 2
                elif mode == 2 and i + 4 < len(tokens):
                    r, g, b = tokens[i + 2], tokens[i + 3], tokens[i + 4]
                    col = (max(0, min(255, r)), max(0, min(255, g)), max(0, min(255, b)))
                    if is_fg:
                        self.fg = col
                    else:
                        self.bg = col
                    i += 4
            i += 1

    def row_text(self, r: int) -> str:
        return "".join(cell.ch for cell in self.grid[r])

    def find_text(self, substr: str) -> Optional[tuple[int, int]]:
        for r in range(self.rows):
            txt = self.row_text(r)
            idx = txt.find(substr)
            if idx != -1:
                return (idx + 1, r + 1)  # 1-based terminal coordinates
        return None


# ---------------------------------------------------------------------------
# 3. High-DPI Frame Compositor with Keystroke Overlay & Narrative Subtitle
# ---------------------------------------------------------------------------


@dataclass
class KeyEventDisplay:
    label: str
    action: str
    press_time: float


class Compositor:
    def __init__(self) -> None:
        self.font_reg = ImageFont.truetype(MENLO_PATH, 16, index=0)
        self.font_bold = ImageFont.truetype(MENLO_PATH, 16, index=1)
        self.font_ui_bold = ImageFont.truetype(SANS_PATH, 20)
        self.font_ui_sm = ImageFont.truetype(SANS_REG_PATH, 16)
        self.font_title = ImageFont.truetype(SANS_PATH, 26)
        self.font_cmd = ImageFont.truetype(MENLO_PATH, 16, index=0)
        self.font_sub = ImageFont.truetype(SANS_REG_PATH, 23)
        self.font_key_big = ImageFont.truetype(SANS_PATH, 28)
        self.font_key_trail = ImageFont.truetype(SANS_PATH, 18)

        self.term_img = Image.new("RGB", (COLS * CELL_W, ROWS * CELL_H), BG_TERM)
        self.term_draw = ImageDraw.Draw(self.term_img)
        self._last_cells: list[list[Optional[tuple]]] = [[None] * COLS for _ in range(ROWS)]
        self._glyph_cache: dict[tuple[str, tuple[int, int, int], bool], Image.Image] = {}

    def reset(self) -> None:
        self.term_draw.rectangle([0, 0, COLS * CELL_W - 1, ROWS * CELL_H - 1], fill=BG_TERM)
        self._last_cells = [[None] * COLS for _ in range(ROWS)]

    def _draw_geometric_cell(
        self, x0: int, y0: int, ch: str, fg: tuple[int, int, int], bg: tuple[int, int, int]
    ) -> bool:
        code = ord(ch)
        d = self.term_draw
        w, h = CELL_W, CELL_H
        # Braille U+2800..U+28FF (2x4 grid, 7x6 px dots with 1px gap)
        if 0x2800 <= code <= 0x28FF:
            bits = code - 0x2800
            dot_bits = [
                (0, 0, 0x01), (0, 1, 0x02), (0, 2, 0x04), (0, 3, 0x40),
                (1, 0, 0x08), (1, 1, 0x10), (1, 2, 0x20), (1, 3, 0x80),
            ]
            dw, dh = w // 2, h // 4  # 7 x 6
            for dx, dy, mask in dot_bits:
                if bits & mask:
                    px = x0 + dx * dw + 1
                    py = y0 + dy * dh + 1
                    d.rectangle([px, py, px + dw - 2, py + dh - 2], fill=fg)
            return True

        # Exact integer Block Elements (U+2580..U+259F)
        if ch == "█":
            d.rectangle([x0, y0, x0 + w - 1, y0 + h - 1], fill=fg)
            return True
        if ch == "▀":
            d.rectangle([x0, y0, x0 + w - 1, y0 + h // 2 - 1], fill=fg)
            return True
        if ch == "▄":
            d.rectangle([x0, y0 + h // 2, x0 + w - 1, y0 + h - 1], fill=fg)
            return True
        if ch == "▌":
            d.rectangle([x0, y0, x0 + w // 2 - 1, y0 + h - 1], fill=fg)
            return True
        if ch == "▐":
            d.rectangle([x0 + w // 2, y0, x0 + w - 1, y0 + h - 1], fill=fg)
            return True
        lower_eighths = {"▁": 1, "▂": 2, "▃": 3, "▄": 4, "▅": 5, "▆": 6, "▇": 7}
        if ch in lower_eighths:
            k = lower_eighths[ch]
            bar_h = max(1, round((h * k) / 8))
            d.rectangle([x0, y0 + h - bar_h, x0 + w - 1, y0 + h - 1], fill=fg)
            return True
        left_eighths = {"▏": 1, "▎": 2, "▍": 3, "▌": 4, "▋": 5, "▊": 6, "▉": 7}
        if ch in left_eighths:
            k = left_eighths[ch]
            bar_w = max(1, (w * k) // 8)
            d.rectangle([x0, y0, x0 + bar_w - 1, y0 + h - 1], fill=fg)
            return True
        if ch in ("░", "▒", "▓"):
            alpha = 0.25 if ch == "░" else 0.50 if ch == "▒" else 0.75
            mix = tuple(int(bg[i] * (1 - alpha) + fg[i] * alpha) for i in range(3))
            d.rectangle([x0, y0, x0 + w - 1, y0 + h - 1], fill=mix)
            return True

        # Crisp pixel-perfect Box Drawing (aligns tracks without subpixel seams)
        mx = x0 + w // 2
        my = y0 + h // 2
        thick = 2 if ch in ("━", "┃", "┏", "┓", "┗", "┛", "═", "║") else 2
        box_arms = {
            "─": (1, 1, 0, 0), "━": (1, 1, 0, 0), "│": (0, 0, 1, 1), "┃": (0, 0, 1, 1),
            "┌": (0, 1, 0, 1), "╭": (0, 1, 0, 1), "┐": (1, 0, 0, 1), "╮": (1, 0, 0, 1),
            "└": (0, 1, 1, 0), "╰": (0, 1, 1, 0), "┘": (1, 0, 1, 0), "╯": (1, 0, 1, 0),
            "├": (0, 1, 1, 1), "┤": (1, 0, 1, 1), "┬": (1, 1, 0, 1), "┴": (1, 1, 1, 0),
            "┼": (1, 1, 1, 1), "═": (1, 1, 0, 0), "║": (0, 0, 1, 1),
        }
        if ch in box_arms:
            l, r, u, dn = box_arms[ch]
            if l:
                d.rectangle([x0, my - 1, mx, my + thick - 2], fill=fg)
            if r:
                d.rectangle([mx, my - 1, x0 + w - 1, my + thick - 2], fill=fg)
            if u:
                d.rectangle([mx - 1, y0, mx + thick - 2, my], fill=fg)
            if dn:
                d.rectangle([mx - 1, my, mx + thick - 2, y0 + h - 1], fill=fg)
            return True
        return False

    def sync_terminal(self, screen: VT100Screen) -> None:
        for r in range(ROWS):
            for c in range(COLS):
                cell = screen.grid[r][c]
                fg, bg = (cell.bg, cell.fg) if cell.rev else (cell.fg, cell.bg)
                if cell.dim:
                    fg = tuple(max(25, int(v * 0.65)) for v in fg)
                key = (cell.ch, fg, bg, cell.bold)
                if self._last_cells[r][c] == key:
                    continue
                self._last_cells[r][c] = key
                x0 = c * CELL_W
                y0 = r * CELL_H
                self.term_draw.rectangle([x0, y0, x0 + CELL_W - 1, y0 + CELL_H - 1], fill=bg)
                if cell.ch != " ":
                    if not self._draw_geometric_cell(x0, y0, cell.ch, fg, bg):
                        font = self.font_bold if cell.bold else self.font_reg
                        self.term_draw.text((x0 + 1, y0 + 1), cell.ch, font=font, fill=fg)

    def render_frame(
        self,
        screen: VT100Screen,
        title: str,
        cli_hint: str,
        step_idx: int,
        step_total: int,
        narrative: str,
        progress: float,
        recent_keys: list[KeyEventDisplay],
        now_t: float,
    ) -> Image.Image:
        self.sync_terminal(screen)
        img = Image.new("RGB", (FRAME_W, FRAME_H), BG_CANVAS)
        draw = ImageDraw.Draw(img)

        # 1. Top Header Bar
        draw.rectangle([0, 0, FRAME_W, 60], fill=(16, 22, 34))
        draw.line([0, 60, FRAME_W, 60], fill=BORDER_WIN, width=2)
        draw.rounded_rectangle([32, 14, 134, 48], radius=8, fill=(14, 116, 144))
        draw.text((46, 20), "igv-rs", font=self.font_ui_bold, fill=(240, 249, 255))
        draw.text((152, 16), title, font=self.font_title, fill=(248, 250, 252))

        # CLI badge on right
        cmd_box = draw.textbbox((0, 0), cli_hint, font=self.font_cmd)
        cw = cmd_box[2] - cmd_box[0] + 28
        cx0 = FRAME_W - 32 - cw
        draw.rounded_rectangle([cx0, 14, FRAME_W - 32, 48], radius=8, fill=(26, 34, 49), outline=(56, 72, 96), width=1)
        draw.text((cx0 + 14, 21), cli_hint, font=self.font_cmd, fill=(148, 210, 255))

        # 2. Terminal Window Chrome & Viewport
        draw.rounded_rectangle(
            [WIN_X0, WIN_Y0, WIN_X1, WIN_Y1],
            radius=12,
            fill=BG_TERM,
            outline=BORDER_WIN,
            width=2,
        )
        draw.rounded_rectangle([WIN_X0 + 1, WIN_Y0 + 1, WIN_X1 - 1, WIN_Y0 + 36], radius=11, fill=BG_CHROME)
        draw.rectangle([WIN_X0 + 1, WIN_Y0 + 22, WIN_X1 - 1, WIN_Y0 + 37], fill=BG_CHROME)
        draw.line([WIN_X0, WIN_Y0 + 37, WIN_X1, WIN_Y0 + 37], fill=(40, 52, 70), width=1)
        for idx, col in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
            cx = WIN_X0 + 22 + idx * 20
            cy = WIN_Y0 + 18
            draw.ellipse([cx - 6, cy - 6, cx + 6, cy + 6], fill=col)
        draw.text(
            (WIN_X0 + 88, WIN_Y0 + 10),
            f"Terminal ({COLS}x{ROWS}) — igv-rs live PTY session",
            font=self.font_ui_sm,
            fill=(148, 163, 184),
        )
        img.paste(self.term_img, (GRID_X, GRID_Y))

        # 3. Floating Key-Caster Badge inside bottom-right of terminal frame whenever recent key is active
        active_key: Optional[KeyEventDisplay] = recent_keys[-1] if recent_keys else None
        if active_key is not None and (now_t - active_key.press_time) <= 2.2:
            age = now_t - active_key.press_time
            is_flash = age < 0.32
            border_c = (251, 191, 36) if is_flash else ACCENT_CYAN
            key_bg = (245, 158, 11) if is_flash else (30, 41, 59)
            key_fg = (15, 20, 29) if is_flash else (254, 243, 199)

            kb = draw.textbbox((0, 0), active_key.label, font=self.font_key_big)
            kw = max(66, (kb[2] - kb[0]) + 34)
            ab = draw.textbbox((0, 0), active_key.action, font=self.font_ui_bold)
            aw = (ab[2] - ab[0]) + 28
            hud_w = max(300, kw + aw + 40)
            hud_h = 88
            hx1 = WIN_X1 - 18
            hx0 = hx1 - hud_w
            hy1 = WIN_Y1 - 8
            hy0 = hy1 - hud_h

            draw.rounded_rectangle(
                [hx0, hy0, hx1, hy1],
                radius=12,
                fill=(18, 25, 39),
                outline=border_c,
                width=3,
            )
            draw.text((hx0 + 16, hy0 + 8), "KEY PRESSED", font=self.font_cmd, fill=border_c)
            draw.text((hx0 + 16 + kw + 16, hy0 + 44), active_key.action, font=self.font_ui_bold, fill=(241, 245, 249))

            # 3D keycap pill
            kx0, ky0 = hx0 + 16, hy0 + 33
            kx1, ky1 = kx0 + kw, ky0 + 42
            draw.rounded_rectangle([kx0, ky0 + 4, kx1, ky1 + 4], radius=7, fill=(10, 14, 22))
            draw.rounded_rectangle([kx0, ky0, kx1, ky1], radius=7, fill=key_bg, outline=border_c, width=2)
            tw = kb[2] - kb[0]
            draw.text((kx0 + (kw - tw) // 2, ky0 + 6), active_key.label, font=self.font_key_big, fill=key_fg)

        # 4. Persistent On-Screen Keystroke Trail Bar (y = 886..946)
        draw.rounded_rectangle(
            [32, 886, 1888, 946],
            radius=10,
            fill=(20, 27, 40),
            outline=(45, 58, 80),
            width=2,
        )
        draw.rounded_rectangle([44, 898, 174, 934], radius=7, fill=(30, 58, 95))
        draw.text((57, 905), "KEY TRAIL", font=self.font_ui_bold, fill=(186, 230, 253))

        tx = 190
        shown_keys = recent_keys[-12:]
        for idx, kev in enumerate(shown_keys):
            is_last = idx == len(shown_keys) - 1
            age = now_t - kev.press_time
            fresh = is_last and age < 1.6
            bb = draw.textbbox((0, 0), kev.label, font=self.font_key_trail)
            pw = max(36, (bb[2] - bb[0]) + 22)
            if tx + pw > 1420:
                break
            fill_c = (245, 158, 11) if (is_last and age < 0.35) else (29, 78, 216) if fresh else (34, 44, 62)
            out_c = (253, 224, 71) if fresh else (71, 85, 105)
            txt_c = (15, 20, 29) if (is_last and age < 0.35) else (255, 255, 255) if fresh else (203, 213, 225)
            draw.rounded_rectangle([tx, 899, tx + pw, 935], radius=6, fill=(10, 14, 22))
            draw.rounded_rectangle([tx, 896, tx + pw, 932], radius=6, fill=fill_c, outline=out_c, width=2 if fresh else 1)
            draw.text((tx + (pw - (bb[2] - bb[0])) // 2, 904), kev.label, font=self.font_key_trail, fill=txt_c)
            tx += pw + 8

        if active_key is not None:
            action_str = f"Action: {active_key.action}"
            ab2 = draw.textbbox((0, 0), action_str, font=self.font_ui_bold)
            aw2 = ab2[2] - ab2[0]
            draw.text((max(1435, 1870 - aw2), 905), action_str, font=self.font_ui_bold, fill=ACCENT_EMERALD)

        # 5. Narrative Subtitle Banner (y = 954..1062) + Progress Bar (y = 1070..1076)
        draw.rounded_rectangle(
            [32, 954, 1888, 1062],
            radius=10,
            fill=(17, 24, 39),
            outline=(51, 65, 85),
            width=2,
        )
        draw.rounded_rectangle([32, 954, 42, 1062], radius=4, fill=ACCENT_CYAN)
        draw.rounded_rectangle([58, 966, 210, 996], radius=6, fill=(22, 48, 74))
        draw.text((68, 971), f"Step {step_idx} / {step_total}", font=self.font_cmd, fill=(125, 211, 252))

        # Word-wrap narrative up to 2 lines
        words = narrative.split()
        lines = []
        cur = ""
        for w in words:
            cand = f"{cur} {w}".strip()
            if draw.textlength(cand, font=self.font_sub) <= 1590:
                cur = cand
            else:
                lines.append(cur)
                cur = w
        if cur:
            lines.append(cur)
        for li, line in enumerate(lines[:2]):
            draw.text((230, 967 + li * 38), line, font=self.font_sub, fill=(248, 250, 252))

        # Progress bar
        draw.rectangle([32, 1070, 1888, 1076], fill=(30, 41, 59))
        pw = int((1888 - 32) * max(0.0, min(1.0, progress)))
        if pw > 0:
            draw.rectangle([32, 1070, 32 + pw, 1076], fill=ACCENT_CYAN)

        return img


# ---------------------------------------------------------------------------
# 4. PTY Automation Runner + Spoken Audio Muxing
# ---------------------------------------------------------------------------


KEY_DISPLAY_MAP = {
    "\t": "Tab",
    "\r": "Enter",
    "\n": "Enter",
    "\x1b": "Esc",
    " ": "Space",
    "\x1b[D": "Left",
    "\x1b[C": "Right",
    "\x1b[A": "Up",
    "\x1b[B": "Down",
}


def format_key_label(seq: str) -> str:
    if seq in KEY_DISPLAY_MAP:
        return KEY_DISPLAY_MAP[seq]
    if len(seq) == 1 and "A" <= seq <= "Z":
        return f"Shift + {seq}"
    return seq


@dataclass
class TimedEvent:
    kind: str  # "key", "type", "click", "custom", "wait"
    payload: str = ""
    action: str = ""
    key_label: Optional[str] = None
    delay_after: float = 0.75
    fn: Optional[Callable[[VT100Screen], None]] = None


@dataclass
class Scene:
    narration: str
    events: list[TimedEvent] = field(default_factory=list)
    poster_capture: bool = False


@dataclass
class VideoSpec:
    slug: str
    title: str
    cli_args: list[str]
    cli_hint: str
    scenes: list[Scene]
    socket_port: Optional[int] = None


def synthesize_wav(text: str, out_wav: Path) -> float:
    aiff = out_wav.with_suffix(".aiff")
    subprocess.run(
        ["/usr/bin/say", "-v", "Samantha", "-r", "185", "-o", str(aiff), text],
        check=True,
    )
    subprocess.run(
        ["ffmpeg", "-y", "-v", "error", "-i", str(aiff), "-ac", "1", "-ar", "44100", "-sample_fmt", "s16", str(out_wav)],
        check=True,
    )
    aiff.unlink(missing_ok=True)
    with wave.open(str(out_wav), "rb") as w:
        return w.getnframes() / float(w.getframerate())


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def drain_pty(master_fd: int, screen: VT100Screen, duration: float) -> None:
    end = time.monotonic() + duration
    while True:
        rem = end - time.monotonic()
        if rem <= 0:
            break
        r, _, _ = select.select([master_fd], [], [], min(0.03, rem))
        if r:
            try:
                chunk = os.read(master_fd, 65536)
                if chunk:
                    screen.feed_bytes(chunk)
                else:
                    break
            except OSError:
                break


def record_video(spec: VideoSpec, compositor: Compositor) -> dict:
    compositor.reset()
    VIDEO_DIR.mkdir(parents=True, exist_ok=True)
    mp4_path = VIDEO_DIR / f"{spec.slug}.mp4"
    gif_path = VIDEO_DIR / f"{spec.slug}.gif"
    poster_path = VIDEO_DIR / f"{spec.slug}-poster.png"

    with tempfile.TemporaryDirectory(prefix=f"igv_{spec.slug}_") as tmp:
        tmp_dir = Path(tmp)
        work_dir = tmp_dir / "work"
        work_dir.mkdir()
        (work_dir / "examples").symlink_to(ROOT / "examples")

        # Substitute `{tmp}` placeholders in CLI args if any
        cmd = [str(BIN_PATH)] + [a.replace("{tmp}", str(work_dir)) for a in spec.cli_args]
        if "--no-browser" not in cmd:
            cmd.append("--no-browser")

        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        env = os.environ.copy()
        env["TERM"] = "xterm-256color"
        env["COLORTERM"] = "truecolor"
        env["COLUMNS"] = str(COLS)
        env["LINES"] = str(ROWS)

        def make_ctty() -> None:
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        proc = subprocess.Popen(
            cmd,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            cwd=str(work_dir),
            preexec_fn=make_ctty,
            close_fds=True,
        )
        os.close(slave)

        screen = VT100Screen(COLS, ROWS)
        # Allow tracks to finish async loading & initial render
        drain_pty(master, screen, 0.95)

        # Pre-generate speech WAVs and measure exact durations per scene
        scene_wavs: list[tuple[Path, float]] = []
        for idx, sc in enumerate(spec.scenes):
            wpath = tmp_dir / f"speech_{idx:02d}.wav"
            dur = synthesize_wav(sc.narration, wpath)
            scene_wavs.append((wpath, dur))

        total_scenes = len(spec.scenes)
        # First expand all scenes into discrete micro-steps so we know exact total timeline
        full_wav_path = tmp_dir / "full_audio.wav"
        mp4_silent = tmp_dir / "silent.mp4"

        ff = subprocess.Popen(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-s",
                f"{FRAME_W}x{FRAME_H}",
                "-r",
                str(FPS),
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-preset",
                "medium",
                "-crf",
                "20",
                str(mp4_silent),
            ],
            stdin=subprocess.PIPE,
        )
        assert ff.stdin is not None

        recent_keys: list[KeyEventDisplay] = []
        timeline_t = 0.0
        poster_saved = False

        # Estimate total duration up front for clean progress bar
        est_total = sum(
            max(dur + 0.45, sum(
                (len(ev.payload) * 0.11 + ev.delay_after) if ev.kind == "type" else ev.delay_after
                for ev in sc.events
            ) + 0.35)
            for (_, dur), sc in zip(scene_wavs, spec.scenes)
        )

        # Build concatenated PCM track synced sample-accurately to emitted frame count
        pcm_chunks: list[bytes] = []
        emitted_frames = 0

        for sc_idx, (sc, (wpath, speech_dur)) in enumerate(zip(spec.scenes, scene_wavs), start=1):
            with wave.open(str(wpath), "rb") as w:
                speech_pcm = w.readframes(w.getnframes())
            scene_start_frame = emitted_frames

            def emit_duration(secs: float, capture_poster_here: bool = False) -> None:
                nonlocal timeline_t, emitted_frames, poster_saved
                n_frames = max(1, round(secs * FPS))
                last_bytes: Optional[bytes] = None
                for fi in range(n_frames):
                    drain_pty(master, screen, 0.02 if fi == 0 else 0.0)
                    # Redraw whenever key flash boundary, screen update, or fade expires
                    if fi == 0 or screen.dirty or fi in (8, 9, 32, 38) or (fi % 6 == 0):
                        pil_img = compositor.render_frame(
                            screen=screen,
                            title=spec.title,
                            cli_hint=spec.cli_hint,
                            step_idx=sc_idx,
                            step_total=total_scenes,
                            narrative=sc.narration,
                            progress=timeline_t / max(est_total, 1.0),
                            recent_keys=recent_keys,
                            now_t=timeline_t,
                        )
                        if capture_poster_here and not poster_saved and fi >= 2:
                            pil_img.save(poster_path, format="PNG")
                            poster_saved = True
                        last_bytes = pil_img.tobytes()
                        screen.dirty = False
                    assert last_bytes is not None
                    ff.stdin.write(last_bytes)
                    emitted_frames += 1
                    timeline_t += 1.0 / FPS

            scene_action_time = 0.0
            for ev in sc.events:
                payload = ev.payload.replace("{tmp}", str(work_dir))
                if ev.kind == "key":
                    os.write(master, payload.encode("utf-8"))
                    lbl = ev.key_label or format_key_label(payload)
                    recent_keys.append(KeyEventDisplay(lbl, ev.action, timeline_t))
                    drain_pty(master, screen, 0.14)
                    hold = max(0.20, ev.delay_after - 0.14)
                    emit_duration(hold, capture_poster_here=sc.poster_capture)
                    scene_action_time += ev.delay_after
                elif ev.kind == "type":
                    per_char = 0.055 if len(payload) > 20 else 0.085
                    for ch in payload:
                        os.write(master, ch.encode("utf-8"))
                        lbl = format_key_label(ch)
                        recent_keys.append(KeyEventDisplay(lbl, ev.action, timeline_t))
                        drain_pty(master, screen, 0.035)
                        emit_duration(per_char, capture_poster_here=False)
                        scene_action_time += per_char
                    if ev.delay_after > 0:
                        drain_pty(master, screen, 0.18)
                        emit_duration(ev.delay_after, capture_poster_here=sc.poster_capture)
                        scene_action_time += ev.delay_after
                elif ev.kind == "click":
                    # Find exact target character (`!` Mendelian violation) or track border
                    coords = screen.find_text(payload)
                    if coords is not None and payload == "!":
                        x, y = coords[0], coords[1]
                    elif coords is not None:
                        x, y = coords[0] + 18, min(ROWS - 2, coords[1] + 1)
                    else:
                        x, y = 48, 18
                    sgr = f"\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m".encode("ascii")
                    os.write(master, sgr)
                    recent_keys.append(KeyEventDisplay(ev.key_label or "Click", ev.action, timeline_t))
                    drain_pty(master, screen, 0.22)
                    emit_duration(ev.delay_after, capture_poster_here=sc.poster_capture)
                    scene_action_time += ev.delay_after
                elif ev.kind == "custom" and ev.fn is not None:
                    ev.fn(screen)
                    if ev.key_label:
                        recent_keys.append(KeyEventDisplay(ev.key_label, ev.action, timeline_t))
                    drain_pty(master, screen, 0.25)
                    emit_duration(ev.delay_after, capture_poster_here=sc.poster_capture)
                    scene_action_time += ev.delay_after
                elif ev.kind == "wait":
                    emit_duration(ev.delay_after, capture_poster_here=sc.poster_capture)
                    scene_action_time += ev.delay_after

            target_dur = speech_dur + 0.45
            if scene_action_time < target_dur:
                emit_duration(target_dur - scene_action_time, capture_poster_here=False)

            scene_frames = emitted_frames - scene_start_frame
            target_samples = round((scene_frames / FPS) * 44100)
            target_bytes = target_samples * 2
            if len(speech_pcm) >= target_bytes:
                pcm_chunks.append(speech_pcm[:target_bytes])
            else:
                pcm_chunks.append(speech_pcm + b"\x00" * (target_bytes - len(speech_pcm)))

        if not poster_saved:
            compositor.render_frame(
                screen=screen,
                title=spec.title,
                cli_hint=spec.cli_hint,
                step_idx=total_scenes,
                step_total=total_scenes,
                narrative=spec.scenes[-1].narration,
                progress=1.0,
                recent_keys=recent_keys,
                now_t=timeline_t,
            ).save(poster_path, format="PNG")

        ff.stdin.close()
        ff.wait(timeout=30)
        os.close(master)
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.terminate()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=2)

        with wave.open(str(full_wav_path), "wb") as out_w:
            out_w.setnchannels(1)
            out_w.setsampwidth(2)
            out_w.setframerate(44100)
            out_w.writeframes(b"".join(pcm_chunks))

        # Mux silent video + synced speech audio into faststart H.264/AAC MP4
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-i",
                str(mp4_silent),
                "-i",
                str(full_wav_path),
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "96k",
                "-movflags",
                "+faststart",
                "-shortest",
                str(mp4_path),
            ],
            check=True,
        )

        # Create crisp 960x540 @ 10fps animated GIF preview via palettegen/paletteuse
        palette_png = tmp_dir / "palette.png"
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-i",
                str(mp4_path),
                "-vf",
                "fps=10,scale=960:540:flags=lanczos,palettegen=stats_mode=diff",
                str(palette_png),
            ],
            check=True,
        )
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-i",
                str(mp4_path),
                "-i",
                str(palette_png),
                "-lavfi",
                "fps=10,scale=960:540:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3",
                str(gif_path),
            ],
            check=True,
        )

    probe = subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "format=duration,size:stream=codec_name,width,height",
            "-of",
            "default=noprint_wrappers=1",
            str(mp4_path),
        ],
        text=True,
    ).strip()
    return {
        "slug": spec.slug,
        "mp4_mb": mp4_path.stat().st_size / (1024 * 1024),
        "gif_mb": gif_path.stat().st_size / (1024 * 1024),
        "probe": probe.replace("\n", " | "),
    }


# ---------------------------------------------------------------------------
# 5. Seven Feature Video Scenarios
# ---------------------------------------------------------------------------


def build_video_specs() -> list[VideoSpec]:
    fa = "examples/demo/demo.fa"
    gtf = "examples/demo/genes.gtf"
    sam = "examples/demo/reads.sam"
    vcf = "examples/demo/trio.vcf"
    ped = "examples/demo/trio.ped"
    wig = "examples/demo/chip_h3k27ac.wig"
    bg = "examples/demo/atac_seq.bedGraph"
    bedpe = "examples/demo/loops.bedpe"
    seg = "examples/demo/cohort_cnv.seg"
    gwas = "examples/demo/cardiometabolic.gwas"

    port7 = find_free_port()

    def send_tcp_goto(port: int, locus: str) -> Callable[[VT100Screen], None]:
        def _run(_screen: VT100Screen) -> None:
            for _ in range(10):
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=1.0) as s:
                        s.sendall(f"goto {locus}\n".encode("ascii"))
                        _ = s.recv(1024)
                    return
                except OSError:
                    time.sleep(0.1)
        return _run

    return [
        # Video 1
        VideoSpec(
            slug="01-navigation-and-panes",
            title="1. Navigation, Bookmarks & Multi-Locus Split Panes",
            cli_args=[fa, "-g", gtf, "-s", wig, "-r", "chr1:60-320"],
            cli_hint="igv-rs examples/demo/demo.fa -g genes.gtf -s chip_h3k27ac.wig -r chr1:60-320",
            scenes=[
                Scene(
                    "Launch igv-rs with reference sequence, GTF genes, and ChIP-seq signal tracks across the BRCA1 locus.",
                    [TimedEvent("wait", delay_after=1.4)],
                ),
                Scene(
                    "Press d to page forward a full window, a to page back, and l or h for smooth one-tenth fine panning.",
                    [
                        TimedEvent("key", "d", "Page Forward (Full Window)", delay_after=0.9),
                        TimedEvent("key", "a", "Page Backward (Full Window)", delay_after=0.9),
                        TimedEvent("key", "l", "Fine Pan Right (1/10)", delay_after=0.7),
                        TimedEvent("key", "h", "Fine Pan Left (1/10)", delay_after=0.7),
                    ],
                ),
                Scene(
                    "Press w to zoom in toward base-level resolution, and s to zoom back out.",
                    [
                        TimedEvent("key", "w", "Zoom In Around Center", delay_after=0.9),
                        TimedEvent("key", "s", "Zoom Out Around Center", delay_after=0.9),
                    ],
                ),
                Scene(
                    "Open the command palette with colon, type TP53, and press Enter to jump straight to that gene.",
                    [
                        TimedEvent("type", ":TP53\r", "Jump to Gene Name (:TP53)", delay_after=1.4),
                    ],
                ),
                Scene(
                    "Press m then a to save bookmark a, pan away with d, then press single-quote a to snap right back.",
                    [
                        TimedEvent("key", "m", "Bookmark Mode (awaiting key)", key_label="m", delay_after=0.45),
                        TimedEvent("key", "a", "Save Bookmark 'a'", key_label="a", delay_after=0.8),
                        TimedEvent("key", "d", "Page Forward", delay_after=0.8),
                        TimedEvent("key", "'", "Jump to Bookmark Mode", key_label="'", delay_after=0.45),
                        TimedEvent("key", "a", "Jump to Bookmark 'a'", key_label="a", delay_after=1.0),
                    ],
                ),
                Scene(
                    "Run colon split chr2:80-680 to open EGFR side-by-side in a split pane, press Tab to cycle panes, and unsplit when finished.",
                    [
                        TimedEvent("type", ":split chr2:80-680\r", "Open Split Pane 2 (chr2 EGFR)", delay_after=1.6),
                        TimedEvent("key", "\t", "Cycle Active Split Pane", key_label="Tab", delay_after=1.2),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Press question mark for built-in keybinding help, Escape to close overlays, and q to exit cleanly.",
                    [
                        TimedEvent("key", "?", "Toggle Keybinding Help", key_label="?", delay_after=1.2),
                        TimedEvent("key", "\x1b", "Close Overlay", key_label="Esc", delay_after=0.7),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 2
        VideoSpec(
            slug="02-alignment-analytics",
            title="2. Alignment Coloring, Sorting, Pairing & Methylation Mods",
            cli_args=[fa, "-b", sam, "-g", gtf, "-r", "chr1:100-270"],
            cli_hint="igv-rs demo.fa -b reads.sam -g genes.gtf -r chr1:100-270",
            scenes=[
                Scene(
                    "Open paired-end and modified reads aligned over BRCA1 with SNV allele-fraction bars in the coverage track.",
                    [
                        TimedEvent("key", "+", "Grow Alignment Pane Height", key_label="+", delay_after=0.8),
                        TimedEvent("key", "j", "Scroll Alignment Lanes Down", key_label="j", delay_after=0.7),
                        TimedEvent("key", "k", "Scroll Alignment Lanes Up", key_label="k", delay_after=0.7),
                    ],
                ),
                Scene(
                    "Press Shift P to toggle View-as-Pairs, joining forward and reverse mates onto shared horizontal lanes.",
                    [
                        TimedEvent("key", "P", "Toggle View-as-Pairs", delay_after=1.7),
                    ],
                ),
                Scene(
                    "Press Shift C repeatedly to cycle coloring modes from strand to insert size, pair orientation, mate chromosome, MAPQ, MM/ML 5mC methylation, and bisulfite.",
                    [
                        TimedEvent("key", "C", "Color Mode -> insert", delay_after=0.85),
                        TimedEvent("key", "C", "Color Mode -> pair", delay_after=0.85),
                        TimedEvent("key", "C", "Color Mode -> mapq", delay_after=0.85),
                        TimedEvent("key", "C", "Color Mode -> mod (MM/ML 5mC)", delay_after=1.2),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Type colon sort base and Enter to stratify reads by base call at the heterozygous G over A SNV site.",
                    [
                        TimedEvent("type", ":sort base\r", "Sort Alignments by Center Base", delay_after=1.6),
                    ],
                ),
                Scene(
                    "Group haplotypes with colon group hp, reveal soft-clipped bases with colon softclip on, and press q to finish.",
                    [
                        TimedEvent("type", ":group hp\r", "Group Reads by HP Haplotype", delay_after=1.4),
                        TimedEvent("type", ":softclip on\r", "Show Soft-Clipped Overhangs", delay_after=1.4),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 3
        VideoSpec(
            slug="03-sequence-and-motifs",
            title="3. Sequence Strand, 3-Frame Translation & IUPAC Motifs",
            cli_args=[fa, "-g", gtf, "-r", "chr1:115-265"],
            cli_hint="igv-rs demo.fa -g genes.gtf -r chr1:115-265",
            scenes=[
                Scene(
                    "Zoom into per-base nucleotide resolution using w so individual bases appear along the reference track.",
                    [
                        TimedEvent("key", "w", "Zoom In to Base Resolution", delay_after=1.3),
                    ],
                ),
                Scene(
                    "Press Shift T to display live three-frame amino-acid translation tracks with green start and red stop codons.",
                    [
                        TimedEvent("key", "T", "Toggle 3-Frame AA Translation", delay_after=1.7),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Use colon strand both to add the complementary reverse strand, and colon translate rev to inspect reverse-strand reading frames.",
                    [
                        TimedEvent("type", ":strand both\r", "Show Complementary DNA Strand", delay_after=1.4),
                        TimedEvent("type", ":translate rev\r", "Translate Reverse 3 Frames", delay_after=1.4),
                    ],
                ),
                Scene(
                    "Run IUPAC motif searches across both strands using colon motif TATAWAW and colon motif GAATTC, then quit with q.",
                    [
                        TimedEvent("type", ":motif TATAWAW\r", "Highlight IUPAC Motif TATAWAW", delay_after=1.5),
                        TimedEvent("type", ":motif GAATTC\r", "Highlight EcoRI Motif GAATTC", delay_after=1.5),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 4
        VideoSpec(
            slug="04-variants-and-pedigree",
            title="4. Multi-Sample Genotypes, PED Trios, Inspector & Themes",
            cli_args=[fa, "-v", vcf, "--ped", ped, "-g", gtf, "-r", "chr1:90-350"],
            cli_hint="igv-rs demo.fa -v trio.vcf --ped trio.ped -g genes.gtf",
            scenes=[
                Scene(
                    "Load a family trio VCF alongside a PED pedigree file; samples automatically sort Father, Mother, Proband and flag Mendelian violations with red bangs.",
                    [
                        TimedEvent("key", "l", "Fine Pan Right Across Trio", delay_after=0.9),
                        TimedEvent("key", "h", "Fine Pan Left Across Trio", delay_after=0.9),
                    ],
                ),
                Scene(
                    "Click directly on any variant row in the VCF track to open the interactive modal Inspector popup showing genotypes and Mendelian alerts, then press Escape to dismiss.",
                    [
                        TimedEvent("click", "!", "Click Mendelian Variant -> Inspector", key_label="Click", delay_after=2.0),
                        TimedEvent("key", "\x1b", "Close Variant Inspector", key_label="Esc", delay_after=0.8),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Press t repeatedly to cycle through igv-rs color themes including light, paper, dracula, and gruvbox, then quit with q.",
                    [
                        TimedEvent("key", "t", "Theme -> light", delay_after=0.9),
                        TimedEvent("key", "t", "Theme -> paper", delay_after=0.9),
                        TimedEvent("key", "t", "Theme -> dracula", delay_after=0.9),
                        TimedEvent("key", "t", "Theme -> gruvbox-dark", delay_after=0.9),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 5
        VideoSpec(
            slug="05-sashimi-signals-and-links",
            title="5. Sashimi Splice Junctions, Signal Tracks & 3D Links",
            cli_args=[fa, "-b", sam, "-g", gtf, "-s", wig, "-s", bg, "-l", bedpe, "-r", "chr1:60-520"],
            cli_hint="igv-rs demo.fa -b reads.sam -g genes.gtf -s *.wig -l loops.bedpe",
            scenes=[
                Scene(
                    "Combine RNA-seq alignments, H3K27ac and ATAC quantitative signals, and BEDPE chromatin interaction arcs across BRCA1.",
                    [TimedEvent("wait", delay_after=1.2)],
                ),
                Scene(
                    "Press Shift J to toggle RNA-seq Sashimi plots, aggregating intron skips into labeled splice-junction arcs.",
                    [
                        TimedEvent("key", "J", "Toggle Sashimi Splice Arcs", delay_after=1.8),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Resize signal and link panes with closing brace and greater-than, toggle shared Y-scale with backslash, and change summary aggregation.",
                    [
                        TimedEvent("key", "}", "Grow Signal Track Height", key_label="}", delay_after=0.8),
                        TimedEvent("key", ">", "Grow Link Track Height", key_label=">", delay_after=0.8),
                        TimedEvent("key", "\\", "Toggle Shared Signal Y-Scale", key_label="\\", delay_after=1.0),
                        TimedEvent("type", ":summary mean\r", "Signal Summary -> Mean", delay_after=1.4),
                    ],
                ),
                Scene(
                    "Pan across BRCA1 and TP53 with d and a, then press q to exit.",
                    [
                        TimedEvent("key", "d", "Page Forward", delay_after=0.9),
                        TimedEvent("key", "a", "Page Back", delay_after=0.9),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 6
        VideoSpec(
            slug="06-cnv-gwas-and-snapshots",
            title="6. CNV Heatmaps, GWAS Manhattan & Vector/Circular Export",
            cli_args=[fa, "--seg", seg, "--gwas", gwas, "-l", bedpe, "-g", gtf, "-r", "chr1:1-1000"],
            cli_hint="igv-rs demo.fa --seg cohort_cnv.seg --gwas cardiometabolic.gwas -l loops.bedpe",
            scenes=[
                Scene(
                    "View multi-sample SEG copy-number diverging heatmaps stacked directly above a Braille GWAS Manhattan plot.",
                    [
                        TimedEvent("key", "w", "Zoom Into GWAS Peak", delay_after=1.0),
                        TimedEvent("key", "d", "Pan Forward Along Chromosome", delay_after=1.0),
                        TimedEvent("key", "a", "Pan Back to Peak", delay_after=1.0),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Press Shift S to export an instant vector SVG snapshot of the active viewport, or specify custom PNG output via colon snapshot.",
                    [
                        TimedEvent("key", "S", "Save Instant SVG Snapshot", delay_after=1.3),
                        TimedEvent("type", ":snapshot panel.png\r", "Export Raster PNG Figure", delay_after=1.5),
                    ],
                ),
                Scene(
                    "Generate a whole-genome Circos-style SV and CNV chord diagram using colon circular, then quit with q.",
                    [
                        TimedEvent("type", ":circular circos.svg\r", "Export Circular Chord SVG", delay_after=1.5),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
        # Video 7
        VideoSpec(
            slug="07-sessions-batch-and-automation",
            title="7. Live Sessions, Browser Sync & TCP Socket Automation",
            cli_args=[fa, "-g", gtf, "-v", vcf, "--port", str(port7), "--no-browser", "-r", "chr1:60-360"],
            cli_hint=f"igv-rs demo.fa -g genes.gtf -v trio.vcf --port {port7}",
            socket_port=port7,
            scenes=[
                Scene(
                    "Start igv-rs with a live IGV TCP command listener enabled on port zero, ready for interactive or scripted control.",
                    [TimedEvent("wait", delay_after=1.2)],
                ),
                Scene(
                    "Dynamically load an ATAC-seq track without restarting using colon load, and save the workspace state with colon save-session.",
                    [
                        TimedEvent("type", f":load {bg}\r", "Hot-Load ATAC-seq Track", delay_after=1.5),
                        TimedEvent("type", ":save-session work.toml\r", "Save TOML Session File", delay_after=1.4),
                    ],
                ),
                Scene(
                    "Press Shift B to launch the embedded igv.js web companion with bidirectional locus synchronization.",
                    [
                        TimedEvent("key", "B", "Start igv.js Browser Server", delay_after=1.5),
                    ],
                ),
                Scene(
                    "External tools can steer the live TUI over TCP by sending goto coordinates or snapshot commands to the listening socket.",
                    [
                        TimedEvent(
                            "custom",
                            action=f"TCP :{port7} -> goto chr1:620-1080",
                            key_label="TCP",
                            delay_after=1.6,
                            fn=send_tcp_goto(port7, "chr1:620-1080"),
                        ),
                    ],
                    poster_capture=True,
                ),
                Scene(
                    "Clean up tracks live with colon unload atac_seq, and exit with q.",
                    [
                        TimedEvent("type", ":unload atac_seq\r", "Unload Track by Name", delay_after=1.3),
                        TimedEvent("key", "q", "Quit igv-rs", key_label="q", delay_after=0.8),
                    ],
                ),
            ],
        ),
    ]


def main() -> None:
    import sys

    build_demo_fixtures()
    compositor = Compositor()
    specs = build_video_specs()
    only = sys.argv[1:]
    if only:
        specs = [s for s in specs if any(tok in s.slug for tok in only)]
    results = []
    for spec in specs:
        print(f"Recording {spec.slug} ...", flush=True)
        res = record_video(spec, compositor)
        results.append(res)
        print(
            f"  -> mp4={res['mp4_mb']:.2f} MB | gif={res['gif_mb']:.2f} MB | {res['probe']}",
            flush=True,
        )
    print(f"Generated {len(results)} feature video(s) successfully.")


if __name__ == "__main__":
    main()
