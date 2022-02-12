use clap::Parser;
use futures::{stream, StreamExt};
use reqwest::Client;
use std::{
    fmt,
    fs::{self, File},
    io::Write,
    path::Path,
    str::FromStr,
};

#[derive(Debug)]
enum PointFormat {
    GKOT,
    OTR,
    DTM,
}

impl fmt::Display for PointFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PointFormat::GKOT => write!(f, "gkot"),
            PointFormat::OTR => write!(f, "otr"),
            PointFormat::DTM => write!(f, "dmr1"),
        }
    }
}

impl FromStr for PointFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match &s[..] {
            "gkot" => Ok(PointFormat::GKOT),
            "otr" => Ok(PointFormat::OTR),
            "dtm" => Ok(PointFormat::DTM),
            _ => Err(format!("Unknown point format: {}", s)),
        }
    }
}

#[derive(Debug)]
enum FileFormat {
    ZLAS,
    LAZ,
    ASC,
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FileFormat::ZLAS => write!(f, "zlas"),
            FileFormat::LAZ => write!(f, "laz"),
            FileFormat::ASC => write!(f, "asc"),
        }
    }
}

impl FromStr for FileFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match &s[..] {
            "zlas" => Ok(FileFormat::ZLAS),
            "laz" => Ok(FileFormat::LAZ),
            "asc" => Ok(FileFormat::ASC),
            _ => Err(format!("Unknown file format: {}", s)),
        }
    }
}

#[derive(Debug)]
struct AreaCode {
    letter: char,
    number: u32,
}

impl fmt::Display for AreaCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}_{}", self.letter, self.number)
    }
}

impl FromStr for AreaCode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let letter = chars
            .next()
            .expect("Area code must have a letter")
            .to_lowercase()
            .next()
            .expect("Area code must have a letter");
        let number = chars
            .collect::<String>()
            .parse::<u32>()
            .expect("Invalid number");
        Ok(AreaCode { letter, number })
    }
}

#[derive(Debug)]
enum CoordinateSystem {
    D96TM,
    D48GK,
}

impl fmt::Display for CoordinateSystem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CoordinateSystem::D96TM => write!(f, "D96TM"),
            CoordinateSystem::D48GK => write!(f, "D48GK"),
        }
    }
}

impl FromStr for CoordinateSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match &s[..] {
            "d96tm" => Ok(CoordinateSystem::D96TM),
            "d48gk" => Ok(CoordinateSystem::D48GK),
            _ => Err("Unknown coordinate system".to_string()),
        }
    }
}

#[derive(Debug)]
struct Coordinate<'a> {
    x: u64,
    y: u64,
    system: Option<&'a CoordinateSystem>,
    point_format: Option<&'a PointFormat>,
}

impl<'a> fmt::Display for Coordinate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let (Some(system), Some(point_format)) = (self.system, self.point_format) {
            let coordinate_system = match system {
                CoordinateSystem::D96TM => "TM",
                CoordinateSystem::D48GK => "GK",
            };

            let format = match *point_format {
                PointFormat::GKOT => "",
                PointFormat::OTR => "R",
                PointFormat::DTM => "1",
            };

            write!(f, "{}{}_{}_{}", coordinate_system, format, self.x, self.y)
        } else {
            panic!("Coordinate system not specified");
        }
    }
}

impl<'a> FromStr for Coordinate<'a> {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('_');

        let x = parts
            .next()
            .expect("No x coordinate")
            .parse::<u64>()
            .expect("X coordinate is not a number");
        let y = parts
            .next()
            .expect("No y coordinate")
            .parse::<u64>()
            .expect("Y coordinate is not a number");
        Ok(Coordinate {
            x,
            y,
            system: None,
            point_format: None,
        })
    }
}

#[derive(Parser, Debug)]
#[clap(author = "Luka Pršina", version = "0.1.0", about = None, long_about = None)]
struct Args<'a> {
    /// GKOT, OTR or DTM
    #[clap(short, long)]
    point_format: PointFormat,

    /// ZLAS, LAZ or ASC
    #[clap(short, long)]
    file_format: FileFormat,

    /// example: b14
    #[clap(short, long)]
    area_code: AreaCode,

    /// D96TM or D48GK
    #[clap(short = 's', long, default_value = "D96TM")]
    coordinate_system: CoordinateSystem,

    /// first coordinate x_y
    #[clap(short = '1', long)]
    first_coord: Coordinate<'a>,

    /// second coordinate x_y
    #[clap(short = '2', long)]
    second_coord: Coordinate<'a>,
}

const CONCURRENT_REQUESTS: usize = 2;

struct Link<'a> {
    url: String,

    point_format: &'a PointFormat,

    #[allow(dead_code)]
    file_format: &'a FileFormat,

    #[allow(dead_code)]
    area_code: &'a AreaCode,

    #[allow(dead_code)]
    coordinate_system: &'a CoordinateSystem,

    coordinate: Coordinate<'a>,
}

impl<'a> Link<'a> {
    fn new(
        point_format: &'a PointFormat,
        file_format: &'a FileFormat,
        area_code: &'a AreaCode,
        coordinate_system: &'a CoordinateSystem,
        coordinate: Coordinate<'a>,
    ) -> Self {
        Link {
            url: format!(
                "http://gis.arso.gov.si/lidar/{}/{}/{}/{}.{}",
                point_format, area_code, coordinate_system, &coordinate, file_format
            ),
            point_format,
            file_format,
            area_code,
            coordinate_system,
            coordinate,
        }
    }
}

// http://gis.arso.gov.si/lidar/gkot/b_14/D96TM/TM_510_74.zlas
// cargo run -- -p gkot -c 510_74 -f zlas -a b14
#[tokio::main]
async fn main() {
    let output = Path::new("output");
    fs::create_dir_all(output).expect("Failed to create output directory");

    let mut args = Args::parse();
    args.first_coord.system = Some(&args.coordinate_system);
    args.second_coord.system = Some(&args.coordinate_system);

    let client = Client::new();

    let mut links = vec![];

    for x in args.first_coord.x..=args.second_coord.x {
        for y in args.first_coord.y..=args.second_coord.y {
            let link = Link::new(
                &args.point_format,
                &args.file_format,
                &args.area_code,
                &args.coordinate_system,
                Coordinate {
                    x,
                    y,
                    system: Some(&args.coordinate_system),
                    point_format: Some(&args.point_format),
                },
            );

            println!("{}", link.url);
            links.push(link);
        }
    }

    let client = &client;

    let bodies = stream::iter(&links)
        .map(|link| {
            let client = client.clone();
            async move {
                let response = client
                    .get(&link.url)
                    .send()
                    .await
                    .expect(&format!("Failed to get file from link {}", &link.url));
                response.bytes().await
            }
        })
        .buffer_unordered(CONCURRENT_REQUESTS);

    {
        let links = &links;

        bodies
            .enumerate()
            .for_each(|(pos, body)| async move {
                let link = &links[pos];

                let path = output.join(format!(
                    "{}_{}.{}",
                    &link.coordinate.x, &link.coordinate.y, &link.point_format
                ));
                let mut file = File::create(&path).expect("Failed to create file");

                match &body {
                    Ok(b) => file.write_all(&b).expect("Failed to write bytes"),
                    Err(e) => eprintln!("Got an error: {}", e),
                }
            })
            .await;
    }
}
