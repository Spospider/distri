// user interface and peer app would be implemented here
// use an instance of the peer class to perform all interactions
// communicate with fekry & ali on function signatures.

use std::io::{self, Write};
use std::collections::HashMap;
use distri::peer::Peer;
use std::sync::Arc;


fn clear_screen() {
    print!("\x1B[2J\x1B[H"); // ANSI escape codes to clear the screen and move the cursor to the top-left corner
    io::stdout().flush().unwrap();
}

fn render_ui(
    directory_of_service: &Vec<(&str, Vec<&str>)>,
    received_requests: &Vec<(&str, &str, usize)>,
    pending_requests: &Vec<(String, String, usize)>,
    granted_access: &HashMap<(String, String), usize>,
) {
    clear_screen();
    println!("===== Peer-to-Peer Image Sharing =====\n");

    // Directory of Service
    println!("Directory of Service:");
    for (user, images) in directory_of_service {
        println!("- {}: {:?}", user, images);
    }

    // Pending Requests
    println!("\nPending Requests (Sent):");
    for (to_user, image, views) in pending_requests {
        println!("- To {}: Image: {}, Requested Views: {}", to_user, image, views);
    }

    // Received Requests
    println!("\nReceived Requests:");
    for (from_user, image, views) in received_requests {
        println!("- From {}: Image: {}, Requested Views: {}", from_user, image, views);
    }

    // Granted Access
    println!("\nGranted Access:");
    for ((from_user, image), views_left) in granted_access {
        println!("- From {}: Image: {}, Remaining Views: {}", from_user, image, views_left);
    }

    // Actions
    println!("\nActions:");
    println!("[1] Refresh View");
    println!("[2] Request Image");
    println!("[3] Accept/Reject Requests");
    println!("[4] View Image");
    println!("[q] Quit");
    println!("\nEnter your choice:");
}

pub async fn run_program(peer:&Arc<Peer>) {
    // peer is passed
    peer.start().await.unwrap();
    let directory_of_service = vec![
        ("User A", vec!["image1", "image2"]),
        ("User B", vec!["image3"]),
    ];

    let mut received_requests = vec![("User B", "image1", 5)];
    let mut pending_requests = vec![("User C".to_string(), "image4".to_string(), 3)];
    let mut granted_access: HashMap<(String, String), usize> = HashMap::new();
    granted_access.insert(("User B".to_string(), "image1".to_string()), 3);

    loop {
        render_ui(
            &directory_of_service,
            &received_requests,
            &pending_requests,
            &granted_access,
        );

        // Read user input
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let choice = input.trim();

        match choice {
            "1" => {
                // Refresh view
                println!("Refreshing view...");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            "2" => {
                // Request image
                println!("Enter the username to request an image from:");
                let mut username = String::new();
                io::stdin().read_line(&mut username).unwrap();
                let username = username.trim().to_string();

                println!("Enter the image name:");
                let mut image_name = String::new();
                io::stdin().read_line(&mut image_name).unwrap();
                let image_name = image_name.trim().to_string();

                println!("Enter the number of views needed:");
                let mut views = String::new();
                io::stdin().read_line(&mut views).unwrap();
                let views = views.trim().parse::<usize>().unwrap_or(0);

                pending_requests.push((username, image_name, views));
                println!("Request sent.");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            "3" => {
                // Accept/Reject requests
                println!("Select a request to process:");
                for (i, (from_user, image, views)) in received_requests.iter().enumerate() {
                    println!(
                        "[{}] From {}: Image: {}, Requested Views: {}",
                        i + 1,
                        from_user,
                        image,
                        views
                    );
                }

                let mut request_choice = String::new();
                io::stdin().read_line(&mut request_choice).unwrap();

                if let Ok(index) = request_choice.trim().parse::<usize>() {
                    if index > 0 && index <= received_requests.len() {
                        let (from_user, image, views) = received_requests[index - 1];
                        println!(
                            "Accept, Reject, or Accept with Updated Views? (a/r/u):"
                        );

                        let mut decision = String::new();
                        io::stdin().read_line(&mut decision).unwrap();

                        match decision.trim() {
                            "a" => {
                                granted_access.insert((from_user.to_string(), image.to_string()), views);
                                println!("Accepted request!");
                            }
                            "r" => {
                                println!("Rejected request!");
                            }
                            "u" => {
                                println!("Enter the updated number of views:");
                                let mut updated_views = String::new();
                                io::stdin().read_line(&mut updated_views).unwrap();
                                if let Ok(new_views) = updated_views.trim().parse::<usize>() {
                                    // granted_access.insert((from_user.to_string(), image.to_string()), new_views);
                                    println!(
                                        "Accepted request with updated views: {}",
                                        new_views
                                    );
                                } else {
                                    println!("Invalid number of views.");
                                }
                            }
                            _ => println!("Invalid choice."),
                        }
                        received_requests.remove(index - 1);
                    } else {
                        println!("Invalid selection.");
                    }
                } else {
                    println!("Invalid input.");
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            "4" => {
                // View image
                println!("Enter the username who granted access:");
                let mut username = String::new();
                io::stdin().read_line(&mut username).unwrap();
                let username = username.trim().to_string();

                println!("Enter the image name to view:");
                let mut image_name = String::new();
                io::stdin().read_line(&mut image_name).unwrap();
                let image_name = image_name.trim().to_string();

                let key = (username.clone(), image_name.clone());
                if let Some(views_left) = granted_access.get_mut(&key) {
                    if *views_left > 0 {
                        *views_left -= 1;
                        println!("Viewing image '{}' from '{}'.", image_name, username);
                    } else {
                        println!("No remaining views for this image.");
                    }
                } else {
                    println!("Access not granted for this image.");
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            "q" => {
                println!("Exiting...");
                break;
            }
            _ => {
                println!("Invalid choice. Try again.");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
}