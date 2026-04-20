use std::path::PathBuf;

use clap::Parser;
use petro_chunked::chunks::ChunksIter;
use petro_chunked::content::DataOrMiniChunks;
use petro_chunked::{ChunkContent, iter_chunks};

#[derive(Parser)]
struct Args {
    /// The ALO file to parse.
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let content = std::fs::read(&args.file)?;
    print_chunked_indent(iter_chunks(&content), 0)?;
    Ok(())
}

fn print_chunked_indent(chunks: ChunksIter, indent: usize) -> anyhow::Result<()> {
    for chunk in chunks {
        let chunk = chunk?;
        print_indent(indent);
        println!("Found chunk with ID 0x{:0>8x}", chunk.id());
        match chunk.content() {
            ChunkContent::Data(data) => print_data_or_mini(data, indent + 4),
            ChunkContent::Chunks(children) => print_chunked_indent(children, indent + 4)?,
        }
    }
    Ok(())
}

fn print_indent(indent: usize) {
    for _ in 0..indent {
        print!(" ");
    }
}

fn print_data_or_mini(data: DataOrMiniChunks, indent: usize) {
    let mini_chunks = data.as_mini_chunks().collect::<Result<Vec<_>, _>>();
    match mini_chunks {
        Ok(mini_chunks) if mini_chunks.iter().all(|chunk| chunk.id() != 0) => {
            for chunk in mini_chunks {
                print_indent(indent);
                println!("Mini-Chunk ID 0x{:0>2x}", chunk.id());
                print_data(chunk.content(), indent + 4);
            }
        }
        _ => {
            print_data(&data, indent);
        }
    }
}

fn print_data(data: &[u8], indent: usize) {
    const MAX_WIDTH: usize = 20;
    print_indent(indent);
    print!("[");
    for (i, &b) in data[..data.len().min(MAX_WIDTH)].iter().enumerate() {
        if i != 0 {
            print!(" ");
        }
        print!("{b:0>2x}");
    }
    if data.len() > MAX_WIDTH {
        print!(" ...");
    }
    println!("]");
}
