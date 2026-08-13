use std::process::ExitCode;

use clap::Parser;

mod cli;
mod client;
mod commands;
mod consolidate_cmd;
mod lookup;
mod memory_cmd;
mod output;
mod queries;
mod reattribute_cmd;
mod session_cmd;
mod slots_cmd;
mod timesheet_cmd;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    let args = cli::Cli::parse();
    let code = match args.command {
        cli::Commands::Version => {
            println!("aplan {}", env!("CARGO_PKG_VERSION"));
            output::ExitCode::Success
        }
        cli::Commands::Current => {
            commands::current(&args.api_url, args.json, args.session.as_deref())
        }
        cli::Commands::Sessions => session_cmd::sessions(&args.api_url, args.json),
        cli::Commands::Session { action } => {
            session_cmd::session(&args.api_url, args.json, args.session.as_deref(), &action)
        }
        cli::Commands::Start { task } => {
            commands::start(&args.api_url, args.json, &task, args.session.as_deref())
        }
        cli::Commands::Stop => commands::stop(&args.api_url, args.json, args.session.as_deref()),
        cli::Commands::Flush { task } => {
            commands::flush(&args.api_url, args.json, &task, args.session.as_deref())
        }
        cli::Commands::Note { text, task } => commands::note(
            &args.api_url,
            args.json,
            &text,
            task.as_deref(),
            args.session.as_deref(),
        ),
        cli::Commands::Reattribute {
            from,
            to,
            date,
            since,
            until,
            entry,
            confirm,
        } => reattribute_cmd::reattribute(
            &args.api_url,
            args.json,
            &from,
            &to,
            date.as_deref(),
            since.as_deref(),
            until.as_deref(),
            &entry,
            confirm,
        ),
        cli::Commands::Slots { cmd } => match cmd {
            cli::SlotsCmd::Repair { from, to, confirm } => {
                slots_cmd::repair(&args.api_url, args.json, &from, &to, confirm)
            }
            cli::SlotsCmd::Rebuild { task, date } => {
                slots_cmd::rebuild(&args.api_url, args.json, &task, &date)
            }
        },
        cli::Commands::Log { text, task, at } => commands::log(
            &args.api_url,
            args.json,
            &text,
            task.as_deref(),
            at.as_deref(),
            args.session.as_deref(),
        ),
        cli::Commands::Status { state, task } => commands::status(
            &args.api_url,
            args.json,
            &state,
            task.as_deref(),
            args.session.as_deref(),
        ),
        cli::Commands::Triage { state, task } => {
            commands::triage(&args.api_url, args.json, &state, &task)
        }
        cli::Commands::Done { task, keep_running } => commands::done(
            &args.api_url,
            args.json,
            task.as_deref(),
            args.session.as_deref(),
            keep_running,
        ),
        cli::Commands::Ls { status, triage } => {
            commands::ls(&args.api_url, args.json, &status, &triage)
        }
        cli::Commands::Show { task } => commands::show(&args.api_url, args.json, &task),
        cli::Commands::Dash { date } => commands::dash(&args.api_url, args.json, date.as_deref()),
        cli::Commands::Matrix => commands::matrix(&args.api_url, args.json),
        cli::Commands::Journal { date } => {
            commands::journal(&args.api_url, args.json, date.as_deref())
        }
        cli::Commands::Alerts { all } => commands::alerts(&args.api_url, args.json, all),
        cli::Commands::Rm { task } => commands::rm(&args.api_url, args.json, &task),
        cli::Commands::Priority {
            task,
            urgency,
            impact,
            reset,
        } => commands::priority(
            &args.api_url,
            args.json,
            &task,
            urgency.as_ref(),
            impact.as_ref(),
            reset,
        ),
        cli::Commands::New {
            title,
            deadline,
            urgency,
            impact,
            hours,
        } => commands::new(
            &args.api_url,
            args.json,
            &title,
            deadline.as_deref(),
            urgency.as_ref(),
            impact.as_ref(),
            hours,
        ),
        cli::Commands::Sync { source } => commands::sync(&args.api_url, args.json, source.as_ref()),
        cli::Commands::Resolve { alert } => commands::resolve(&args.api_url, args.json, &alert),
        cli::Commands::Config { cmd } => commands::config(&args.api_url, args.json, &cmd),
        cli::Commands::Timesheet { date, action } => match action {
            None => timesheet_cmd::timesheet(&args.api_url, args.json, date.as_deref()),
            Some(cli::TimesheetAction::Validate) => {
                timesheet_cmd::timesheet_validate(&args.api_url, args.json, date.as_deref())
            }
            Some(cli::TimesheetAction::Set { quarter, task, hours }) => timesheet_cmd::timesheet_set(
                &args.api_url,
                args.json,
                date.as_deref(),
                quarter,
                &task,
                hours,
            ),
            Some(cli::TimesheetAction::Off { am, pm }) => {
                timesheet_cmd::timesheet_off(&args.api_url, args.json, date.as_deref(), am, pm)
            }
        },
        cli::Commands::Map { cmd } => match cmd {
            cli::MapCmd::Add {
                repo,
                branch,
                meeting_subject,
                meeting_organizer,
                internal_project,
                project,
            } => timesheet_cmd::map_add(
                &args.api_url,
                args.json,
                repo.as_deref(),
                branch.as_deref(),
                meeting_subject.as_deref(),
                meeting_organizer.as_deref(),
                internal_project.as_deref(),
                &project,
            ),
            cli::MapCmd::List => timesheet_cmd::map_list(&args.api_url, args.json),
        },
        cli::Commands::Remember {
            title,
            kind,
            why,
            project,
            to,
            task,
            source_ref,
            contradicts,
            confirm,
        } => memory_cmd::remember(
            &args.api_url,
            args.json,
            &title,
            &kind,
            why.as_deref(),
            project.as_deref(),
            &to,
            task.as_deref(),
            args.session.as_deref(),
            source_ref.as_deref(),
            contradicts.as_deref(),
            confirm,
        ),
        cli::Commands::Recall {
            id,
            q,
            history,
            project,
            limit,
        } => match (id, q) {
            (_, Some(query)) => memory_cmd::recall_search(
                &args.api_url,
                args.json,
                &query,
                history,
                project.as_deref(),
                limit,
            ),
            (Some(id), None) => memory_cmd::recall_one(&args.api_url, args.json, &id),
            (None, None) => {
                eprintln!("error: pass a memory id or --q <query>");
                output::ExitCode::PreconditionFailed
            }
        },
        cli::Commands::Brief {
            morning,
            project,
            date,
        } => memory_cmd::brief(
            &args.api_url,
            args.json,
            morning,
            project.as_deref(),
            date.as_deref(),
        ),
        cli::Commands::Inbox { cmd, limit } => match cmd {
            None => memory_cmd::inbox_list(&args.api_url, args.json, limit),
            Some(cli::InboxCmd::Accept { id, kind, force }) => {
                memory_cmd::inbox_accept(&args.api_url, args.json, &id, kind.as_ref(), force)
            }
            Some(cli::InboxCmd::Reject { id }) => {
                memory_cmd::inbox_reject(&args.api_url, args.json, &id)
            }
            Some(cli::InboxCmd::Merge { id, into }) => {
                memory_cmd::inbox_merge(&args.api_url, args.json, &id, &into)
            }
            // `inbox supersede <new> [--replaces <old>]`: the candidate is the
            // successor, and the old memory defaults to the claim it carries.
            Some(cli::InboxCmd::Supersede { id, replaces }) => {
                memory_cmd::supersede(&args.api_url, args.json, replaces.as_deref(), &id)
            }
        },
        cli::Commands::Memory { cmd } => match cmd {
            cli::MemoryCmd::Import { dir } => {
                memory_cmd::memory_import(&args.api_url, args.json, &dir)
            }
            // Outside the queue there is no claim to fall back on: `old` is the
            // required positional argument of this verb.
            cli::MemoryCmd::Supersede { old, by } => {
                memory_cmd::supersede(&args.api_url, args.json, Some(&old), &by)
            }
        },
        cli::Commands::Consolidate { cmd } => match cmd {
            cli::ConsolidateCmd::Pending { limit } => {
                consolidate_cmd::pending(&args.api_url, args.json, limit)
            }
            cli::ConsolidateCmd::Mark { ids } => {
                consolidate_cmd::mark(&args.api_url, args.json, &ids)
            }
            cli::ConsolidateCmd::RecordRun => {
                consolidate_cmd::record_run(&args.api_url, args.json)
            }
        },
    };
    code.into()
}
