#!/usr/bin/env python3

from pathlib import Path
from argparse import ArgumentParser

import datetime
import json
import math

NAMES = ["sha-jN", "sha-j1", "coreutils-1cpu", "coreutils-xargs"]
ALGOS = ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"]


def cmd_stats(res: dict[str, float]) -> dict[str, float | list[float]]:
    """Per-command time + throughput statistics from one hyperfine result."""
    mean: float = res["mean"]
    sd: float = res.get("stddev") or 0.0
    rel: float = sd / mean if mean else 0.0
    g: float = total_gib / mean
    return {
        "time_mean": mean,
        "time_stddev": sd,
        "time_min": res["min"],
        "time_max": res["max"],
        "time_median": res["median"],
        "gibs_mean": g,
        "gibs_stddev": g * rel,
        # min GiB/s <-> slowest run, max GiB/s <-> fastest run
        "gibs_min": total_gib / res["max"],
        "gibs_max": total_gib / res["min"],
        "gibs_median": total_gib / res["median"],
        "times": res.get("times", []),
        "_rel": rel,
    }


def get_float(
    d: dict[str, int | float | list[int | float]],
    key: str,
) -> float:
    item = d[key]
    assert isinstance(item, float)
    return item


def speedup(
    core: dict[str, int | float | list[int | float]],
    sha: dict[str, int | float | list[int | float]],
) -> tuple[float, float]:
    """Throughput speedup of sha over coreutils = t_core / t_sha, with the
    standard ratio error propagation sigma_S/S = sqrt((s_c/t_c)^2+(s_s/t_s)^2)."""
    core_time_mean: float = get_float(core, "time_mean")
    sha_time_mean: float = get_float(sha, "time_mean")
    core_rel: float = get_float(core, "_rel")
    sha_rel: float = get_float(sha, "_rel")
    s: float = core_time_mean / sha_time_mean
    rel: float = math.hypot(core_rel, sha_rel)
    return s, s * rel


def cell(mean: float, sd: float) -> str:
    return f"{mean:.2f}+/-{sd:.2f}"


def spd(pair: tuple[float, float]) -> str:
    return f"{pair[0]:.2f}+/-{pair[1]:.2f}x"


def render(values: list[str | int | float] | list[str], widths: list[int]) -> str:
    return " │ " + " │ ".join(f"{v:<{w}}" for v, w in zip(values, widths)) + " │"


def main(
    workdir: Path,
    total_gib: float,
    total_bytes: int,
    out_json: Path,
    jobs: int,
    reps: int,
    sha_bin: Path,
    num_files: int,
    file_size_mb: int,
    algos: list[str],
    parallel_only: bool,
):
    results: dict[
        str,
        dict[
            str,
            str
            | int
            | float
            | list[str]
            | dict[str, dict[str, float | list[float] | str]],
        ],
    ] = {
        "metadata": {
            "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "sha_bin": str(sha_bin),
            "num_files": num_files,
            "file_size_mb": file_size_mb,
            "total_bytes": total_bytes,
            "total_gib": total_gib,
            "jobs": jobs,
            "reps": reps,
            "parallel_only": parallel_only,
        },
        "algorithms": {},
    }
    if parallel_only:
        col_widths = [8, 16, 16, 15]
        cols = ["algo", f"sha -j{jobs}", "core xargs", "spd/par"]
    else:
        col_widths = [8, 16, 16, 16, 16, 15, 15]
        cols = [
            "algo",
            f"sha -j{jobs}",
            "sha -j1",
            "core 1cpu",
            "core xargs",
            "spd/core",
            "spd/par",
        ]

    print()
    print(
        "All throughput figures are mean±σ in GiB/s over "
        f"{reps} runs; speedups carry propagated uncertainty."
    )
    print()
    print(" ┌─" + "─┬─".join("─" * w for w in col_widths) + "─┐")
    print(render(cols, col_widths))
    print(" ├─" + "─┼─".join("─" * w for w in col_widths) + "─┤")

    for algo in algos:
        with open(workdir / f"{algo}.json") as fh:
            data = json.load(fh)
        by_name: dict[str, dict[str, float | list[float]]] = {
            r["command"]: cmd_stats(r) for r in data["results"]
        }

        jn = by_name["sha-jN"]
        cx = by_name["coreutils-xargs"]

        spd_par = speedup(cx, jn)  # real-world: sha -jN vs coreutils xargs

        jn_mean = get_float(jn, "gibs_mean")
        jn_stddev = get_float(jn, "gibs_stddev")
        cx_mean = get_float(cx, "gibs_mean")
        cx_stddev = get_float(cx, "gibs_stddev")

        if parallel_only:
            row = [
                algo,
                cell(jn_mean, jn_stddev),
                cell(cx_mean, cx_stddev),
                spd(spd_par),
            ]
            names_in_run = ["sha-jN", "coreutils-xargs"]
        else:
            j1 = by_name["sha-j1"]
            c1 = by_name["coreutils-1cpu"]
            spd_core = speedup(c1, j1)  # honest per-core: sha -j1 vs coreutils 1cpu
            j1_mean = get_float(j1, "gibs_mean")
            j1_stddev = get_float(j1, "gibs_stddev")
            c1_mean = get_float(c1, "gibs_mean")
            c1_stddev = get_float(c1, "gibs_stddev")
            row = [
                algo,
                cell(jn_mean, jn_stddev),
                cell(j1_mean, j1_stddev),
                cell(c1_mean, c1_stddev),
                cell(cx_mean, cx_stddev),
                spd(spd_core),
                spd(spd_par),
            ]
            names_in_run = NAMES

        print(render(row, col_widths))

        entry: dict[str, dict[str, float | list[float] | str]] = {
            name: {k: v for k, v in by_name[name].items() if k != "_rel"}
            for name in names_in_run
        }
        entry["speedup_parallel"] = {
            "value": spd_par[0],
            "stddev": spd_par[1],
            "of": "sha-jN vs coreutils-xargs",
        }
        if not parallel_only:
            entry["speedup_per_core"] = {
                "value": spd_core[0],
                "stddev": spd_core[1],
                "of": "sha-j1 vs coreutils-1cpu",
            }
        results["algorithms"][algo] = entry

    print(" └─" + "─┴─".join("─" * w for w in col_widths) + "─┘")

    with open(out_json, "w") as fh:
        json.dump(results, fh, indent=2)
        fh.write("\n")

    print()
    print(f"Full per-run statistics written to {out_json}")


if __name__ == "__main__":
    parser = ArgumentParser(
        description="Render hyperfine results into a human-readable ASCII table and JSON summary.",
        epilog="Should be run in tandem with ./bench-vs-coreutils.sh",
        add_help=True,
        allow_abbrev=True,
        exit_on_error=True,
    )
    parser.add_argument(
        "-w",
        "--workdir",
        action="store",
        type=Path,
        help="directory containing hyperfine JSON results",
        required=True,
        metavar="DIR",
        dest="workdir",
    )
    parser.add_argument(
        "-g",
        "--gib",
        action="store",
        type=float,
        help="total GiB processed in each benchmark run",
        required=True,
        metavar="GIB",
        dest="total_gib",
    )
    parser.add_argument(
        "-b",
        "--bytes",
        action="store",
        type=int,
        help="total bytes processed in each benchmark run",
        required=True,
        metavar="BYTES",
        dest="total_bytes",
    )
    parser.add_argument(
        "-o",
        "--output",
        action="store",
        type=Path,
        help="output JSON file for summary results",
        required=True,
        metavar="OUT_JSON",
        dest="out_json",
    )
    parser.add_argument(
        "-j",
        "--jobs",
        action="store",
        type=int,
        help="total number of jobs used in the parallel benchmark",
        required=True,
        dest="jobs",
    )
    parser.add_argument(
        "-r",
        "--reps",
        action="store",
        type=int,
        help="total number of repetitions for each benchmark",
        required=True,
        dest="reps",
    )
    parser.add_argument(
        "-x",
        "--sha-bin",
        action="store",
        type=Path,
        help="path to the sha binary used in the benchmark",
        required=True,
        metavar="BINARY",
        dest="sha_bin",
    )
    parser.add_argument(
        "-n",
        "--num-files",
        action="store",
        type=int,
        help="total number of files used in the benchmark",
        required=True,
        metavar="FILES",
        dest="num_files",
    )
    parser.add_argument(
        "-s",
        "--file-size",
        action="store",
        type=int,
        help="size of each file used in the benchmark (in MB)",
        required=True,
        metavar="SIZE_MIB",
        dest="file_size_mb",
    )
    parser.add_argument(
        "-p",
        "--parallel-only",
        action="store_true",
        default=False,
        help="only render sha-jN and coreutils-xargs columns; omit per-core data",
        dest="parallel_only",
    )
    parser.add_argument(
        "algos",
        nargs="+",
        type=str,
        choices=ALGOS,
        help="which algorithms to render results for",
        metavar="ALGO",
    )
    args = parser.parse_args()
    workdir = Path(args.workdir).absolute()
    total_gib = float(args.total_gib)
    total_bytes = int(args.total_bytes)
    out_json = Path(args.out_json).absolute()
    jobs = int(args.jobs)
    reps = int(args.reps)
    sha_bin = Path(args.sha_bin).absolute()
    num_files = int(args.num_files)
    file_size_mb = int(args.file_size_mb)
    algos = [str(s) for s in args.algos]
    parallel_only = bool(args.parallel_only)
    main(
        workdir,
        total_gib,
        total_bytes,
        out_json,
        jobs,
        reps,
        sha_bin,
        num_files,
        file_size_mb,
        algos,
        parallel_only,
    )
