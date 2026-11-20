use std::path::Path;
use std::sync::Arc;

use igv_core::pedigree::Pedigree;
use igv_core::region::Region;
use igv_core::source::{
    open_alignment, open_annotation, open_link, open_signal, open_variant, AnnotationFormat,
    FetchOpts, FetchSignalOpts, FileGwasSource, FileSegSource, LinkFormat, NoodlesFastaSource,
    SignalSummary,
};
use igv_core::io::Location;

#[tokio::test]
async fn loads_cram_fixture_if_present() {
    let cram_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/cram/cram_with_crai_index.cram");
    let fa_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/cram/hg19mini.fasta");
    if cram_path.exists() && fa_path.exists() {
        let fasta = Arc::new(NoodlesFastaSource::open(fa_path).await.unwrap());
        let err = open_alignment(cram_path.to_str().unwrap(), fasta, None, None)
            .await
            .err()
            .expect("CRAM 2.1 fixture should report unsupported version");
        assert!(err.to_string().contains("CRAM 2.1"), "unexpected error: {err}");
    }

    // Write and read a real CRAM 3.0 file using tests/data/sample.fa
    use noodles::cram;
    use noodles::fasta as nfasta;
    use noodles::sam::alignment::io::Write as _;
    use noodles::sam::alignment::record::Flags;
    use noodles::sam::alignment::record_buf::{RecordBuf, Sequence as SeqBuf};
    use noodles::sam::header::record::value::map::{ReferenceSequence as RefSeqMap, Map};
    use std::num::NonZeroUsize;

    let sample_fa = Path::new("tests/data/sample.fa");
    let fasta = Arc::new(NoodlesFastaSource::open(sample_fa).await.unwrap());
    let dir = tempfile::tempdir().unwrap();
    let cram3_path = dir.path().join("test.cram");

    let mut fa_reader = nfasta::io::indexed_reader::Builder::default()
        .build_from_path(sample_fa)
        .unwrap();
    let fa_rec = fa_reader.query(&"chr1:1-100".parse().unwrap()).unwrap();
    let seq_slice: Vec<u8> = fa_rec.sequence().as_ref()[..20].to_vec();
    let repo = nfasta::Repository::new(nfasta::repository::adapters::IndexedReader::new(fa_reader));
    let header = noodles::sam::Header::builder()
        .add_reference_sequence(
            b"chr1",
            Map::<RefSeqMap>::new(NonZeroUsize::new(100).unwrap()),
        )
        .build();

    let mut buf = Vec::new();
    let mut writer = cram::io::writer::Builder::default()
        .set_reference_sequence_repository(repo)
        .build_from_writer(&mut buf);
    writer.write_header(&header).unwrap();
    let rec = RecordBuf::builder()
        .set_name(b"cram3_read")
        .set_flags(Flags::default())
        .set_reference_sequence_id(0)
        .set_alignment_start(noodles::core::Position::try_from(1).unwrap())
        .set_mapping_quality(noodles::sam::alignment::record::MappingQuality::try_from(60).unwrap())
        .set_cigar(
            vec![noodles::sam::alignment::record::cigar::Op::new(
                noodles::sam::alignment::record::cigar::op::Kind::Match,
                20,
            )]
            .into_iter()
            .collect(),
        )
        .set_sequence(SeqBuf::from(seq_slice))
        .set_quality_scores(noodles::sam::alignment::record_buf::QualityScores::from(vec![
            30; 20
        ]))
        .build();
    writer.write_alignment_record(&header, &rec).unwrap();
    writer.try_finish(&header).unwrap();
    std::fs::write(&cram3_path, &buf).unwrap();

    let src = open_alignment(cram3_path.to_str().unwrap(), fasta, None, None)
        .await
        .unwrap();
    let rows = src
        .fetch(&Region::new("chr1", 1, 50).unwrap(), &FetchOpts::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].query_name, "cram3_read");
}

#[tokio::test]
async fn loads_bcf_fixture_if_present() {
    use noodles::bcf;
    use noodles::vcf;
    use noodles::vcf::variant::io::Write as _;

    // Write both BGZF-compressed BCF 2.2 and uncompressed BCF 2.2 from a VCF string
    let vcf_text = "\
##fileformat=VCFv4.2
##contig=<ID=20,length=62435964>
##INFO=<ID=AF,Number=A,Type=Float,Description=\"Allele Frequency\">
##FORMAT=<ID=GT,Number=1,Type=String,Description=\"Genotype\">
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\tS2
20\t1500\trs1\tA\tG\t50\tPASS\tAF=0.25\tGT\t0/1\t0/0
";
    let mut vcf_reader = vcf::io::Reader::new(std::io::Cursor::new(vcf_text.as_bytes()));
    let mut header = vcf_reader.read_header().unwrap();
    *header.string_maps_mut() = vcf::header::StringMaps::try_from(&header).unwrap();
    let rec = vcf_reader
        .record_bufs(&header)
        .next()
        .unwrap()
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let bgzf_bcf = dir.path().join("bgzf.bcf");
    {
        let f = std::fs::File::create(&bgzf_bcf).unwrap();
        let mut w = bcf::io::Writer::new(f);
        w.write_header(&header).unwrap();
        w.write_variant_record(&header, &rec).unwrap();
        w.try_finish().unwrap();
    }
    let raw_bcf = dir.path().join("raw.bcf");
    {
        let f = std::fs::File::create(&raw_bcf).unwrap();
        let mut w = bcf::io::Writer::from(f);
        w.write_header(&header).unwrap();
        w.write_variant_record(&header, &rec).unwrap();
    }

    for p in [&bgzf_bcf, &raw_bcf] {
        let src = open_variant(p.to_str().unwrap(), None, None).await.unwrap();
        assert_eq!(src.sample_names(), vec!["S1", "S2"]);
        let recs = src
            .fetch(&Region::new("20", 1000, 2000).unwrap())
            .await
            .unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].genotypes.len(), 2);
        assert_eq!(recs[0].allele_freq, Some(0.25));
    }

    // Legacy BCF 2.1 fixture omitting INFO/FORMAT Description returns a clean error rather than panicking
    let legacy = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/bcf/ex2.bcf");
    if legacy.exists() {
        assert!(open_variant(legacy.to_str().unwrap(), None, None)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn loads_bigbed_and_biginteract_fixtures_if_present() {
    let bb_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/bb/myBigBed2.bb");
    if bb_path.exists() {
        let src = open_annotation(bb_path, Some(AnnotationFormat::BigBed))
            .await
            .unwrap();
        let txs = src
            .fetch(&Region::new("chr7", 50_000, 80_000).unwrap())
            .await
            .unwrap();
        assert!(!txs.is_empty());
    }
    let bi_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/bb/interactExample3.inter.bb");
    if bi_path.exists() {
        let src = open_link(bi_path, Some(LinkFormat::BigInteract))
            .await
            .unwrap();
        let links = src
            .query(&Region::new("chr3", 1, 200_000_000).unwrap(), &Default::default())
            .await
            .unwrap();
        assert!(!links.is_empty());
    }
}

#[tokio::test]
async fn loads_paf_seg_gwas_wig_fixtures_if_present() {
    let paf_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/paf/sample.paf");
    if paf_path.exists() {
        let fasta = Arc::new(
            NoodlesFastaSource::open(Path::new("tests/data/sample.fa"))
                .await
                .unwrap(),
        );
        let src = open_alignment(paf_path.to_str().unwrap(), fasta, None, None)
            .await
            .unwrap();
        let rows = src
            .fetch(&Region::new("KU215903", 1, 125_000).unwrap(), &FetchOpts::default())
            .await
            .unwrap();
        assert!(!rows.is_empty());
    }

    let seg_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/seg/canFam2_hg18.seg");
    if seg_path.exists() {
        let src = FileSegSource::open_location(&Location::Local(seg_path.to_path_buf()), None)
            .await
            .unwrap();
        use igv_core::source::SegSource;
        assert!(!src.samples().is_empty());
        let data = src
            .fetch(&Region::new("chr1", 1, 250_000_000).unwrap())
            .await
            .unwrap();
        assert!(!data.segments.is_empty());
    }

    let gwas_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/gwas/smallp.gwas");
    if gwas_path.exists() {
        let src = FileGwasSource::open_location(&Location::Local(gwas_path.to_path_buf()), None)
            .await
            .unwrap();
        use igv_core::source::GwasSource;
        let pts = src
            .fetch(&Region::new("6", 29_000_000, 30_000_000).unwrap())
            .await
            .unwrap();
        assert!(!pts.is_empty());
    }

    let wig_path = Path::new("/Users/zhanxw/Downloads/igv-main/test/data/wig/test_fixedStep.wig");
    if wig_path.exists() {
        let src = open_signal(wig_path, None).await.unwrap();
        let bins = src
            .fetch(
                &Region::new("chr19", 1, 100_000_000).unwrap(),
                &FetchSignalOpts {
                    max_bins: 100,
                    summary: SignalSummary::Mean,
                },
            )
            .await
            .unwrap();
        assert!(!bins.is_empty());
    }
}

#[tokio::test]
async fn parses_sam_and_unindexed_vcf_with_pedigree_mendelian_check() {
    let mut sam = tempfile::Builder::new().suffix(".sam").tempfile().unwrap();
    std::io::Write::write_all(
        &mut sam,
        b"@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:100\nr1\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tRG:Z:rg1\tHP:i:1\tMM:Z:C+m?,0;\tML:B:C,250\n",
    )
    .unwrap();
    let fasta = Arc::new(
        NoodlesFastaSource::open(Path::new("tests/data/sample.fa"))
            .await
            .unwrap(),
    );
    let src = open_alignment(sam.path().to_str().unwrap(), fasta, Some("RG"), None)
        .await
        .unwrap();
    let rows = src
        .fetch(&Region::new("chr1", 1, 100).unwrap(), &FetchOpts::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tag, Some(("RG".into(), "rg1".into())));
    assert_eq!(rows[0].haplotype, Some(1));
    assert_eq!(rows[0].base_mods.len(), 1);
    assert_eq!(rows[0].base_mods[0].q_idx, 1);

    let mut vcf = tempfile::Builder::new().suffix(".vcf").tempfile().unwrap();
    std::io::Write::write_all(
        &mut vcf,
        b"##fileformat=VCFv4.2\n##FORMAT=<ID=GT,Number=1,Type=String,Description=\"GT\">\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tfather\tmother\tchild\nchr1\t50\trs1\tA\tG\t99\tPASS\t.\tGT\t0/0\t0/0\t0/1\n",
    )
    .unwrap();
    let ped = Arc::new(Pedigree::parse("FAM\tfather\t0\t0\t1\t1\nFAM\tmother\t0\t0\t2\t1\nFAM\tchild\tfather\tmother\t1\t2\n"));
    let vsrc = open_variant(vcf.path().to_str().unwrap(), Some(ped), None)
        .await
        .unwrap();
    assert_eq!(vsrc.sample_names(), vec!["child", "mother", "father"]);
    let recs = vsrc
        .fetch(&Region::new("chr1", 1, 100).unwrap())
        .await
        .unwrap();
    assert_eq!(recs.len(), 1);
    assert!(recs[0].genotypes[0].mendelian_violation);
    assert!(!recs[0].genotypes[1].mendelian_violation);
}
